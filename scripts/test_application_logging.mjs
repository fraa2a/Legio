import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { artworkPacket, createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
Object.defineProperty(globalThis, "window", { configurable: true, value: dom.window });
for (const key of ["document", "navigator", "HTMLElement", "HTMLInputElement", "HTMLImageElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "CustomEvent", "MutationObserver", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { mount, unmount, flushSync } = await import("svelte");
const { get } = await import("svelte/store");
const settle = () => new Promise(resolve => setImmediate(resolve));
let sequence = 0;

async function fixture(extraMocks = {}) {
  const records = [];
  globalThis.applicationLogInvoke = async (command, args) => {
    assert.equal(command, "report_application_event");
    records.push(args);
  };
  const loader = createModuleLoader({
    "@tauri-apps/api/core": dataModule(`export const invoke = (...args) => globalThis.applicationLogInvoke(...args); // fixture ${++sequence}`),
    ...extraMocks,
  });
  return { records, loader, logger: await import(await loader.load("src/lib/services/application-log.ts")) };
}

test("disabled application logging does not forward renderer errors and re-enabling resumes capture", async () => {
  const { records, logger } = await fixture();
  const stop = logger.installApplicationLogging();
  try {
    logger.setApplicationLoggingEnabled(false);
    window.dispatchEvent(new dom.window.ErrorEvent("error", { error: new Error("Not recorded") }));
    await settle();
    assert.equal(records.length, 0);
    logger.setApplicationLoggingEnabled(true);
    window.dispatchEvent(new dom.window.ErrorEvent("error", { error: new TypeError("Cannot read property") }));
    await settle();
    assert.equal(records.length, 1);
    assert.equal(records[0].event, "renderer_error");
    assert.match(records[0].message, /TypeError: Cannot read property/);
    logger.setApplicationLoggingEnabled(false);
    logger.logApplicationEvent("info", "navigation", "library");
    await settle();
    assert.equal(records.length, 1);
  } finally { stop(); }
});

test("local icon display failures record a static warning without the game's identifier", async () => {
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  const { records, loader, logger } = await fixture({
    "src/lib/i18n": dataModule(storeImport + 'export const language = writable("it"); export const t = value => value;'),
    "src/lib/stores/window-activity": dataModule(storeImport + 'export const windowActive = writable(true);'),
    "src/lib/utils/motion": dataModule("export const fadeDuration = 0;"),
  });
  globalThis.applicationLogInvoke = async (command, args) => {
    if (command === "report_application_event") { records.push(args); return; }
    assert.equal(command, "get_game_icon");
    return artworkPacket({ bytes: [1], contentType: "image/png" });
  };
  logger.setApplicationLoggingEnabled(true);
  const warn = console.warn; console.warn = () => {};
  const stop = logger.installApplicationLogging();
  const component = (await import(await loader.load("src/lib/features/library/GameIcon.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(component, { target, props: { game: {id:"PRIVATE_GAME_IDENTIFIER",name:"Private game",steamAppId:null} } });
  try {
    for (let index = 0; index < 4; index++) { await settle(); flushSync(); }
    assert.ok(target.querySelector("img"));
    target.querySelector("img").dispatchEvent(new Event("error"));
    await settle(); flushSync();
    assert.equal(target.querySelector("img"), null);
    assert.ok(records.some(record => record.event === "console" && record.message.includes("local game icon")));
    assert.ok(!JSON.stringify(records).includes("PRIVATE_GAME_IDENTIFIER"));
    assert.ok(!JSON.stringify(records).includes("Private game"));
  } finally { await unmount(instance); target.remove(); stop(); console.warn = warn; }
});

test("unhandled rejection preserves Error stack without serializing arbitrary objects", async () => {
  const { records, logger } = await fixture();
  logger.setApplicationLoggingEnabled(true);
  const stop = logger.installApplicationLogging();
  try {
    const failure = new Error("Library render failed");
    const event = new dom.window.Event("unhandledrejection");
    Object.defineProperty(event, "reason", { value: failure });
    window.dispatchEvent(event);
    const privateEvent = new dom.window.Event("unhandledrejection");
    Object.defineProperty(privateEvent, "reason", { value: { username: "PRIVATE_USER", token: "SECRET" } });
    window.dispatchEvent(privateEvent);
    await settle();
    assert.equal(records[0].event, "unhandled_rejection");
    assert.match(records[0].stack, /Library render failed/);
    assert.ok(!JSON.stringify(records).includes("PRIVATE_USER"));
    assert.ok(!JSON.stringify(records).includes("SECRET"));
  } finally { stop(); }
});

test("console forwarding preserves original output and does not recursively log transport failures", async () => {
  const { records, logger } = await fixture();
  logger.setApplicationLoggingEnabled(true);
  const previous = console.warn;
  const output = [];
  console.warn = (...args) => output.push(args);
  const stop = logger.installApplicationLogging();
  try {
    const privateObject = { token: "PRIVATE_TOKEN" };
    console.warn("Page transition failed", new Error("Snapshot unavailable"), privateObject);
    await settle();
    assert.equal(output.length, 1);
    assert.equal(output[0][2], privateObject);
    assert.equal(records.length, 1);
    assert.equal(records[0].level, "warn");
    assert.ok(!JSON.stringify(records).includes("PRIVATE_TOKEN"));
    globalThis.applicationLogInvoke = async () => { throw new Error("Transport unavailable"); };
    logger.logApplicationEvent("error", "bootstrap_error", "Failed");
    await settle();
    assert.equal(output.length, 2, "one static warning is emitted without recursively reporting itself");
  } finally { stop(); console.warn = previous; }
});

test("renderer records obey native UTF-8 limits and cap an error storm", async () => {
  const { records, logger } = await fixture();
  logger.setApplicationLoggingEnabled(true);
  const message = "🙂".repeat(2000);
  for (let index = 0; index < 100; index++) logger.logApplicationEvent("error", "renderer_error", message, message);
  await settle();
  assert.equal(records.length, 50);
  for (const record of records) {
    assert.ok(Buffer.byteLength(record.message) <= 1024);
    assert.ok(Buffer.byteLength(record.stack) <= 4096);
    assert.ok(!record.message.includes("�"));
  }
});

test("routine error storms cannot consume the budget reserved for crash diagnostics", async () => {
  const { records, logger } = await fixture();
  logger.setApplicationLoggingEnabled(true);
  for (let index = 0; index < 100; index++) logger.logApplicationEvent("warn", "console", "Artwork unavailable");
  logger.logApplicationEvent("error", "renderer_error", "Library render failed");
  await settle();
  assert.equal(records.filter(record => record.event === "console").length, 50);
  assert.equal(records.filter(record => record.event === "renderer_error").length, 1);
});

test("a native command rejection is recorded without its arguments and preserves the original failure", async () => {
  const { records, loader, logger } = await fixture();
  logger.setApplicationLoggingEnabled(true);
  const failure = new Error("Database unavailable");
  globalThis.applicationLogInvoke = async (command, args) => {
    if (command === "report_application_event") { records.push(args); return; }
    assert.equal(command, "list_games");
    throw failure;
  };
  const { invoke } = await import(await loader.load("src/lib/services/invoke.ts"));
  await assert.rejects(invoke("list_games", { username: "PRIVATE_USER", token: "SECRET" }), error => error === failure);
  await settle();
  assert.equal(records.length, 1);
  assert.equal(records[0].event, "command_error");
  assert.match(records[0].message, /list_games.*Database unavailable/);
  assert.ok(!JSON.stringify(records).includes("PRIVATE_USER"));
  assert.ok(!JSON.stringify(records).includes("SECRET"));
});

test("logging settings default off and application and network toggles persist independently", async () => {
  const { logger } = await fixture();
  logger.setApplicationLoggingEnabled(false);
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  const mocks = {
    "@tauri-apps/api/core": dataModule(`export const invoke = (...args) => globalThis.applicationLogInvoke(...args); // settings ${++sequence}`),
    "src/lib/i18n": dataModule(storeImport + 'export const language = writable("it"); export const t = value => value;'),
    "src/lib/services/appearance": dataModule("export const defaultAppearance = () => ({});"),
    "src/lib/stores/downloads": dataModule("export const installedFolder = { load: async () => {} };"),
    "src/lib/stores/app-info": dataModule(storeImport + 'export const appInfo = writable({data:{platform:"windows"}});'),
    "src/lib/services/game-settings": dataModule("export const getCompatibilityLogsDirectory = async () => null;"),
  };
  const settingsLoader = createModuleLoader(mocks);
  const { settings, defaultSettings } = await import(await settingsLoader.load("src/lib/stores/settings.ts"));
  let persisted = structuredClone(defaultSettings);
  globalThis.applicationLogInvoke = async (command, args) => {
    if (command === "get_settings") return structuredClone(persisted);
    if (command === "save_settings") { persisted = structuredClone(args.settings); return structuredClone(persisted); }
    if (command === "get_network_log_status" || command === "get_application_log_status") {
      return { enabled: command === "get_network_log_status" ? persisted.diagnosticsEnabled : persisted.applicationLoggingEnabled, directory: "C:\\Logs", lastError: null, droppedRecords: 0, pendingRecords: 0 };
    }
    throw new Error(`Unexpected command ${command}`);
  };
  await settings.load();
  assert.equal(get(settings).data.applicationLoggingEnabled, false);
  assert.equal(get(settings).data.diagnosticsEnabled, false);
  const component = (await import(await settingsLoader.load("src/lib/features/settings/AdvancedSettings.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(component, { target });
  const render = async () => { await settle(); flushSync(); await settle(); flushSync(); };
  try {
    await render();
    const toggle = label => [...target.querySelectorAll("label")].find(node => node.textContent.includes(label))?.querySelector("input");
    const application = toggle("Salva log applicativi");
    const network = toggle("Salva log di rete");
    assert.ok(application);
    assert.equal(application.checked, false);
    assert.equal(network.checked, false);
    application.checked = true; application.dispatchEvent(new Event("change", { bubbles: true }));
    await render();
    assert.equal(persisted.applicationLoggingEnabled, true);
    assert.equal(persisted.diagnosticsEnabled, false);
    network.checked = true; network.dispatchEvent(new Event("change", { bubbles: true }));
    await render();
    application.checked = false; application.dispatchEvent(new Event("change", { bubbles: true }));
    await render();
    assert.equal(persisted.applicationLoggingEnabled, false);
    assert.equal(persisted.diagnosticsEnabled, true);
  } finally { await unmount(instance); target.remove(); }
});

test("navigation records library checkpoints without disclosing a selected game identifier", async () => {
  const { records, logger } = await fixture();
  logger.setApplicationLoggingEnabled(true);
  const mocks = {
    "@tauri-apps/api/core": dataModule(`export const invoke = (...args) => globalThis.applicationLogInvoke(...args); // navigation ${++sequence}`),
    "src/lib/utils/motion": dataModule("export const reducedMotion = true; export const pageTransition = () => ({ duration: 0 });"),
  };
  const emptyView = dataModule("export default () => {};");
  for (const [feature, view] of [["home", "HomeView"], ["library", "LibraryView"], ["library", "GameDetailView"], ["store", "StoreView"], ["downloads", "DownloadsView"]]) {
    mocks[`src/lib/features/${feature}/${view}.svelte`] = emptyView;
  }
  const loader = createModuleLoader(mocks);
  const log = await import(await loader.load("src/lib/services/application-log.ts"));
  log.setApplicationLoggingEnabled(true);
  const navigation = await import(await loader.load("src/lib/stores/navigation.ts"));
  const component = (await import(await loader.load("src/lib/components/layout/MainContainer.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(component, { target });
  try {
    flushSync(); await settle(); flushSync();
    navigation.selectSection("library");
    flushSync(); await settle(); flushSync();
    assert.equal(target.querySelector("[data-page]").dataset.page, "library");
    assert.ok(records.some(record => record.event === "navigation" && record.message.includes("library")));
    navigation.openGame("PRIVATE_GAME_IDENTIFIER");
    flushSync(); await settle(); flushSync();
    assert.ok(records.some(record => record.event === "navigation" && record.message.includes("game_details")));
    assert.ok(!JSON.stringify(records).includes("PRIVATE_GAME_IDENTIFIER"));
  } finally { await unmount(instance); target.remove(); }
});
