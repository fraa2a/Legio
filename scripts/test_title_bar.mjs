import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLInputElement", "HTMLTextAreaElement", "Element", "Node", "Text", "Comment", "Event", "KeyboardEvent", "MouseEvent", "FocusEvent", "MutationObserver", "HTMLMediaElement"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
// jsdom has no popover API and keeps a closed [popover] hidden, where nothing can take focus.
HTMLElement.prototype.showPopover = function () { this.style.display = "block"; };
globalThis.matchMedia = () => ({ matches: true });
const { mount, unmount, flushSync } = await import("svelte");
const { get, writable } = await import("svelte/store");
const settle = async () => { for (let i = 0; i < 6; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); } };

const storeImport = `import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))};`;
const i18n = dataModule(`${storeImport} export const language = writable("it"); export const t = (value, _language, values = []) => value.replace(/\\{(\\d+)\\}/g, (_, index) => String(values[Number(index)] ?? ""));`);
const stub = async source => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
// Mocked modules are evaluated once per source, so each fixture lives under its own global for the whole run.
const fromGlobal = (fixture, names) => dataModule(names.map(name => `export const ${name} = globalThis.${fixture}.${name};`).join("\n"));
const button = (target, label) => [...target.querySelectorAll("button")].find(item => item.getAttribute("aria-label") === label || item.textContent.trim() === label);
const key = (target, name) => target.dispatchEvent(new KeyboardEvent("keydown", { key: name, bubbles: true, cancelable: true }));

const modelLoader = createModuleLoader({ "src/lib/i18n": i18n });
const { titleBarPath, summarizeDownloads } = await import(await modelLoader.load("src/lib/components/layout/title-bar-model.ts"));
const { formatRelativeTime } = await import(await modelLoader.load("src/lib/utils/format.ts"));

const names = { a: "Alpha", b: "Beta" };
const path = (location) => titleBarPath(
  { section: "home", gameId: null, storeGame: null, modGame: null, recapOpen: false, ...location },
  id => names[id] ?? null,
  section => `[${section}]`,
  "Recap",
);

test("the path names the page and the page back navigation returns to", () => {
  assert.deepEqual(path({}), { back: null, current: "[home]" });
  assert.deepEqual(path({ section: "library", gameId: "a" }), { back: "[library]", current: "Alpha" });
  assert.deepEqual(path({ section: "store", storeGame: { steamAppId: 1, name: "Shop" } }), { back: "[store]", current: "Shop" });
  assert.deepEqual(path({ section: "mods", modGame: { gameId: "b", returnTo: "mods" } }), { back: "[mods]", current: "Beta" });
  assert.deepEqual(path({ section: "mods", modGame: { gameId: "b", returnTo: "game" } }), { back: "Beta", current: "[mods]" });
});

test("the play recap is a page inside Home with a way back", () => {
  assert.deepEqual(path({ section: "home", recapOpen: true }), { back: "[home]", current: "Recap" });
  // The recap flag only matters on Home.
  assert.deepEqual(path({ section: "library", recapOpen: true }), { back: null, current: "[library]" });
});

test("the path ignores selections of other sections and unknown games", () => {
  // A game page stays selected while another section is shown.
  assert.deepEqual(path({ section: "downloads", gameId: "a" }), { back: null, current: "[downloads]" });
  assert.deepEqual(path({ section: "library", gameId: "gone" }), { back: null, current: "[library]" });
  assert.deepEqual(path({ section: "mods", modGame: { gameId: "gone", returnTo: "game" } }), { back: "[library]", current: "[mods]" });
});

test("download summary weights progress by size and sums speeds", () => {
  assert.equal(summarizeDownloads([]), null);
  assert.deepEqual(summarizeDownloads([
    { sizeBytes: 300, downloadedBytes: 300, speedBps: 10 },
    { sizeBytes: 100, downloadedBytes: 0, speedBps: 5 },
    { sizeBytes: 0, downloadedBytes: 0, speedBps: 0 },
  ]), { count: 3, percent: 75, speedBps: 15 });
});

test("relative times read as now under a minute", () => {
  const now = Date.UTC(2026, 0, 10, 12);
  assert.equal(formatRelativeTime(now - 20000, now, "it"), "ora");
  assert.equal(formatRelativeTime(now - 150000, now, "en"), "2 minutes ago");
  assert.equal(formatRelativeTime(now - 3 * 86400000, now, "en"), "3 days ago");
});

// Modules with the same source share one instance, so every test starts from empty stores.
async function notificationStores(load = createModuleLoader({}).load) {
  const notifications = await import(await load("src/lib/stores/notifications.ts"));
  const toast = await import(await load("src/lib/stores/toast.ts"));
  notifications.clearNotifications();
  for (const notice of get(toast.toast)) toast.dismissToast(notice.id);
  return { notifications, toast };
}

test("the history keeps the newest hundred entries and counts errors apart", async () => {
  const { notifications: history } = await notificationStores();
  for (let index = 0; index < 105; index++) history.recordNotification(`info ${index}`, "info", undefined, index);
  history.recordNotification("broken", "error");
  history.recordNotification("careful", "warning");
  const entries = get(history.notifications);
  assert.equal(entries.length, history.notificationLimit);
  assert.deepEqual(entries.slice(0, 3).map(entry => entry.message), ["careful", "broken", "info 104"]);
  assert.equal(entries.at(-1).message, "info 7");
  assert.deepEqual(get(history.unreadNotifications), { attention: 2, other: 98 });
  history.markNotificationsRead();
  assert.deepEqual(get(history.unreadNotifications), { attention: 0, other: 0 });
  history.recordNotification("later", "success");
  assert.deepEqual(get(history.unreadNotifications), { attention: 0, other: 1 });
  history.clearNotifications();
  assert.deepEqual(get(history.notifications), []);
  assert.deepEqual(get(history.unreadNotifications), { attention: 0, other: 0 });
});

test("a retry runs once and stops once it no longer applies", async () => {
  const { notifications: history } = await notificationStores();
  let runs = 0;
  const first = history.recordNotification("failed", "error", { run: () => runs++ });
  assert.equal(history.retryNotification(first), true);
  assert.equal(history.retryNotification(first), false);
  assert.equal(runs, 1);
  const second = history.recordNotification("failed again", "error", { run: () => runs++ });
  history.dropNotificationRetry(second);
  assert.equal(get(history.notifications)[0].retry, null);
  assert.equal(history.retryNotification(second), false);
  const evicted = history.recordNotification("old", "error", { run: () => runs++ });
  for (let index = 0; index < history.notificationLimit; index++) history.recordNotification("new", "info");
  assert.equal(history.retryNotification(evicted), false);
  assert.equal(runs, 1);
});

test("toasts are recorded once and share their retry with the history", async () => {
  const { notifications: history, toast } = await notificationStores();
  let runs = 0;
  toast.showToast("Saved", "success", { persistent: true });
  toast.showToast("Saved", "success", { persistent: true });
  toast.showToast("Offline", "error", { onRetry: () => runs++, persistent: true });
  assert.deepEqual(get(history.notifications).map(entry => [entry.message, entry.tone]), [["Offline", "error"], ["Saved", "success"]]);
  get(toast.toast).at(-1).onRetry();
  assert.equal(runs, 1);
  assert.equal(get(history.notifications)[0].retry, null, "the history cannot retry what the toast already retried");
});

test("a toast with a technical detail keeps it in the history and stays until dismissed", async () => {
  const { notifications: history, toast } = await notificationStores();
  toast.showToast("Could not save", "error", { detail: "disk full (os error 28)" });
  toast.showToast("Plain", "info", { persistent: true });
  assert.deepEqual(get(history.notifications).map(entry => [entry.message, entry.detail]), [["Plain", null], ["Could not save", "disk full (os error 28)"]]);
  assert.equal(get(toast.toast)[0].detail, "disk full (os error 28)");
});

test("background downloads, updates and source failures enter the history", async () => {
  const downloads = writable({ status: "idle", data: [], error: null });
  const updateState = writable({ version: null });
  const sourceRefreshError = writable(null);
  const retried = [];
  globalThis.notificationEvents = { downloads, updateState, sourceRefreshError, retryJob: async id => { retried.push(id); }, refreshSource: async () => { retried.push("source"); } };
  const { load } = createModuleLoader({
    "src/lib/i18n": i18n,
    "src/lib/stores/downloads": fromGlobal("notificationEvents", ["downloads", "retryJob"]),
    "src/lib/services/app-updater": fromGlobal("notificationEvents", ["updateState"]),
    "src/lib/stores/source": fromGlobal("notificationEvents", ["sourceRefreshError", "refreshSource"]),
  });
  const { notifications: history } = await notificationStores(load);
  const { startNotificationEvents } = await import(await load("src/lib/stores/notification-events.ts"));
  const stop = startNotificationEvents();
  try {
    const job = (id, status, error = null) => ({ id, name: id.toUpperCase(), status, error });
    downloads.set({ status: "ready", data: [job("a", "downloading"), job("b", "installed"), job("c", "failed", "old")], error: null });
    assert.deepEqual(get(history.notifications), [], "outcomes from before the session are not replayed");
    downloads.set({ status: "ready", data: [job("a", "failed", "disk full"), job("b", "installed"), job("c", "failed", "old")], error: null });
    const failure = get(history.notifications)[0];
    assert.deepEqual([failure.message, failure.tone], ["Download di A non riuscito: disk full", "error"]);
    downloads.set({ status: "ready", data: [job("a", "queued"), job("b", "installed")], error: null });
    assert.equal(get(history.notifications)[0].retry, null, "a download retried elsewhere drops the retry");
    downloads.set({ status: "ready", data: [job("a", "installed"), job("b", "installed")], error: null });
    assert.equal(get(history.notifications)[0].message, "A installato.");

    updateState.set({ version: "2.0.0" });
    updateState.set({ version: "2.0.0" });
    sourceRefreshError.set("timeout");
    const messages = get(history.notifications).map(entry => entry.message);
    assert.deepEqual(messages.slice(0, 2), ["Aggiornamento delle sorgenti non riuscito: timeout", "Legio 2.0.0 disponibile."]);
    assert.equal(history.retryNotification(get(history.notifications)[0].id), true);
    assert.deepEqual(retried, ["source"]);
  } finally {
    stop();
  }
});

const fixture = {
  appInfo: writable({ data: { desktopEnvironment: null, platform: "linux" } }),
  games: writable({ data: [{ id: "g1", name: "A very long game name" }] }),
  settings: writable({ status: "ready", data: { onboardingCompleted: true } }),
  updateSettings: async () => {},
  addGameDialogOpen: writable(false),
  libraryPortrait: writable(false),
  libraryQuery: writable(""),
  storeQuery: writable(""),
  catalog: writable({ status: "ready" }),
  runCatalogSearch: async () => {},
  refreshSource: async () => {},
  sourceBusy: writable(false),
  activeDownloads: writable([]),
};
globalThis.titleBarFixture = fixture;

async function titleBar() {
  fixture.libraryQuery.set("");
  fixture.activeDownloads.set([]);
  const { load } = createModuleLoader({
    "src/lib/i18n": i18n,
    "src/lib/components/ui/Icon.svelte": await stub("<span></span>"),
    "src/lib/services/window": dataModule("export const closeWindow = async () => {}, isWindowMaximized = async () => false, minimizeWindow = async () => {}, onWindowResized = async () => () => {}, toggleMaximizeWindow = async () => {};"),
    "src/lib/stores/app-info": fromGlobal("titleBarFixture", ["appInfo"]),
    "src/lib/stores/games": fromGlobal("titleBarFixture", ["games"]),
    "src/lib/stores/settings": fromGlobal("titleBarFixture", ["settings", "updateSettings"]),
    "src/lib/stores/library-ui": fromGlobal("titleBarFixture", ["addGameDialogOpen", "libraryPortrait", "libraryQuery", "storeQuery"]),
    "src/lib/stores/catalog": fromGlobal("titleBarFixture", ["catalog", "runCatalogSearch"]),
    "src/lib/stores/source": fromGlobal("titleBarFixture", ["refreshSource", "sourceBusy"]),
    "src/lib/stores/downloads": fromGlobal("titleBarFixture", ["activeDownloads"]),
  });
  const navigation = await import(await load("src/lib/stores/navigation.ts"));
  const palette = await import(await load("src/lib/stores/command-palette.ts"));
  const { notifications: history, toast } = await notificationStores(load);
  navigation.selectSection("home");
  palette.paletteOpen.set(false);
  const Component = (await import(await load("src/lib/components/layout/TitleBar.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target });
  await settle();
  const destroy = async () => { await unmount(instance); target.remove(); };
  return { fixture, navigation, palette, toast, history, target, destroy, load };
}

test("the title bar shows the path and goes back from its first crumb or Escape", async () => {
  const { navigation, target, destroy } = await titleBar();
  try {
    navigation.openGame("g1"); await settle();
    const heading = target.querySelector("[data-page-heading]");
    assert.equal(heading.textContent.trim(), "A very long game name");
    assert.equal(heading.title, "A very long game name", "a truncated name stays readable");
    assert.ok(target.querySelector("[data-tauri-drag-region] [data-page-heading]"));
    button(target, "Torna a Libreria").click(); await settle();
    assert.equal(get(navigation.selectedGameId), null);
    assert.equal(target.querySelector("[data-page-heading]").textContent.trim(), "Libreria");
    assert.equal(button(target, "Torna a Libreria"), undefined);

    navigation.openStoreGame({ steamAppId: 7, name: "Shop game" }); await settle();
    assert.ok(button(target, "Torna a Store"));
    key(window, "Escape"); await settle();
    assert.equal(get(navigation.selectedStoreGame), null);
  } finally { await destroy(); }
});

test("list views keep filtering from the title bar while other views open the palette", async () => {
  const { fixture, navigation, palette, target, destroy } = await titleBar();
  try {
    navigation.selectSection("library"); await settle();
    const filter = target.querySelector("#title-bar-search");
    assert.ok(filter, "Ctrl+F still finds the library filter");
    assert.equal(target.querySelector("#title-bar-palette"), null);
    filter.value = "knight";
    filter.dispatchEvent(new Event("input", { bubbles: true }));
    assert.equal(get(fixture.libraryQuery), "knight");
    assert.equal(get(palette.paletteOpen), false);

    navigation.selectSection("home"); await settle();
    assert.equal(target.querySelector("#title-bar-search"), null, "Ctrl+F opens the palette where nothing is filtered");
    const launcher = target.querySelector("#title-bar-palette");
    launcher.click();
    assert.equal(get(palette.paletteOpen), true);
    assert.equal(get(palette.paletteSeed), "");
    palette.paletteOpen.set(false);
    launcher.focus();
    key(launcher, "h");
    assert.equal(get(palette.paletteOpen), true);
    assert.equal(get(palette.paletteSeed), "h", "the typed key starts the palette query");
  } finally { await destroy(); }
});

test("download status appears only while downloads run and opens the Downloads view", async () => {
  const { fixture, navigation, target, destroy } = await titleBar();
  try {
    const status = () => [...target.querySelectorAll("button")].find(item => item.getAttribute("aria-label")?.startsWith("Download attivi"));
    assert.equal(status(), undefined);
    fixture.activeDownloads.set([{ sizeBytes: 200, downloadedBytes: 50, speedBps: 2048 }, { sizeBytes: 200, downloadedBytes: 150, speedBps: 0 }]);
    await settle();
    assert.equal(status().getAttribute("aria-label"), "Download attivi: 2, 50%. Apri la sezione Download");
    status().click();
    assert.equal(get(navigation.activeSection), "downloads");
  } finally { await destroy(); }
});

test("the bell counts unread notices and opens an accessible history panel", async () => {
  const { toast, history, target, destroy } = await titleBar();
  try {
    toast.showToast("Saved", "success", { persistent: true });
    toast.showToast("Disk full", "error", { persistent: true });
    await settle();
    const bell = () => target.querySelector('[aria-haspopup="dialog"][aria-controls]');
    assert.equal(bell().getAttribute("aria-label"), "Notifiche, 2 non lette, di cui 1 errori o avvisi");
    assert.equal(bell().getAttribute("aria-expanded"), "false");
    bell().click(); await settle();
    assert.equal(bell().getAttribute("aria-expanded"), "true");
    const panel = document.getElementById(bell().getAttribute("aria-controls"));
    assert.equal(panel.getAttribute("role"), "dialog");
    assert.equal(document.activeElement, panel, "the panel takes the focus");
    assert.deepEqual([...panel.querySelectorAll("li p")].map(item => item.textContent.trim()), ["Nuova: Disk full", "Nuova: Saved"]);
    assert.deepEqual(get(history.unreadNotifications), { attention: 0, other: 0 }, "opening marks the history as read");
    assert.equal(bell().getAttribute("aria-label"), "Notifiche");
    key(panel, "Escape"); await settle();
    assert.equal(bell().getAttribute("aria-expanded"), "false");
    assert.equal(document.activeElement, bell(), "the focus returns to the bell");
    assert.equal(document.getElementById(bell().getAttribute("aria-controls")), null);

    bell().click(); await settle();
    button(target, "Cancella tutto").click(); await settle();
    assert.ok(target.textContent.includes("Nessuna notifica in questa sessione."));
    document.body.click(); await settle();
    assert.equal(bell().getAttribute("aria-expanded"), "false", "a click outside closes the panel");
  } finally { await destroy(); }
});

test("the palette starts from the text typed into the title bar search", async () => {
  globalThis.paletteFixture = { appInfo: writable({ data: { platform: "linux" } }), games: writable({ data: [] }) };
  const { load } = createModuleLoader({
    "src/lib/i18n": i18n,
    "src/lib/components/ui/Icon.svelte": await stub("<span></span>"),
    "src/lib/components/ui/Dialog.svelte": await stub('<script>let { title, children } = $props();</script><section role="dialog" aria-label={title}>{@render children(() => {})}</section>'),
    "src/lib/stores/app-info": fromGlobal("paletteFixture", ["appInfo"]),
    "src/lib/stores/games": fromGlobal("paletteFixture", ["games"]),
    "src/lib/stores/library-ui": dataModule(`${storeImport} export const addGameDialogOpen = writable(false), storeQuery = writable("");`),
  });
  const palette = await import(await load("src/lib/stores/command-palette.ts"));
  palette.openPalette("Ho");
  const Component = (await import(await load("src/lib/features/palette/CommandPalette.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Component, { target });
  try {
    await settle();
    assert.equal(target.querySelector('[role="combobox"]').value, "Ho");
    assert.equal(get(palette.paletteSeed), "", "the seed is used once");
    assert.ok([...target.querySelectorAll('[role="option"]')].some(option => option.textContent.includes("Home")));
  } finally {
    await unmount(instance);
    target.remove();
  }
});
