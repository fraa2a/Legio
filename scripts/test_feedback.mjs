import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLDialogElement", "HTMLMediaElement", "HTMLInputElement", "Element", "Node", "Text", "Comment", "Event", "MouseEvent", "CustomEvent", "MutationObserver", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.requestAnimationFrame = (callback) => setTimeout(callback, 0);
globalThis.cancelAnimationFrame = clearTimeout;
HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };
const { mount, unmount, flushSync } = await import("svelte");
const { get, writable } = await import("svelte/store");
const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
const mocks = {
  "src/lib/i18n": dataModule(storeImport + 'export const language = writable("it"); export const t = (value, _language, args = []) => value.replace(/\\{(\\d+)\\}/g, (_, index) => args[index]);'),
  "src/lib/utils/motion": dataModule("export const reducedMotion = true; export const fadeDuration = 0;"),
};
const { load } = createModuleLoader(mocks);
const notices = await import(await load("src/lib/stores/toast.ts"));
const settle = async () => {
  for (let index = 0; index < 6; index++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); }
};
const clearNotices = () => { for (const notice of get(notices.toast)) notices.dismissToast(notice.id); };

test("notifications deduplicate messages, preserve different tones, dismiss individually and expire after five seconds", async () => {
  const scheduled = [];
  const original = globalThis.setTimeout;
  globalThis.setTimeout = (callback, delay) => { scheduled.push({ callback, delay }); return original(() => {}, 0); };
  try {
    notices.showToast("Saved", "success");
    notices.showToast("Saved", "success");
    notices.showToast("Icon could not be read", "error");
    notices.showToast("Information");
    assert.deepEqual(get(notices.toast).map(notice => notice.tone), ["success", "error", "info"]);
    assert.equal(scheduled.length, 3);
    assert.ok(scheduled.every(timer => timer.delay === 5000));
    notices.dismissToast(get(notices.toast)[1].id);
    assert.equal(get(notices.toast).length, 2);
    for (const timer of scheduled) timer.callback();
    assert.equal(get(notices.toast).length, 0);
  } finally { globalThis.setTimeout = original; clearNotices(); }
});

test("an error notice is emitted outside its panel and retains its retry action", async () => {
  const ErrorBanner = (await import(await load("src/lib/components/ui/ErrorBanner.svelte"))).default;
  const target = document.createElement("div");
  let retried = 0;
  const instance = mount(ErrorBanner, { target, props: { message: "Failed to launch", onRetry: () => retried++ } });
  await settle();
  assert.equal(target.querySelector('[role="alert"]'), null);
  const notification = get(notices.toast).find(notice => notice.message === "Failed to launch");
  assert.equal(notification.tone, "error");
  notification.onRetry();
  assert.equal(retried, 1);
  await unmount(instance);
  clearNotices();
});

test("an imported game with an icon warning closes the add dialog and reports successful addition", async () => {
  const game = { id: "manual", name: "My game", steamAppId: 400, executablePath: "/games/game.exe", steamInstallPath: null };
  const games = writable({ data: [] });
  const scan = writable({ status: "ready", selectedPath: "/games/game.exe", directory: "/games", gameName: "My game", candidates: [], error: null });
  globalThis.feedbackImport = { games, scan };
  mocks["src/lib/stores/games"] = dataModule("export const games = globalThis.feedbackImport.games; export const addGame = async () => {};");
  mocks["src/lib/stores/manual-import"] = dataModule(`
    export const manualImport = globalThis.feedbackImport.scan;
    export const resetManualImport = () => {};
    export const setScanGameName = gameName => manualImport.update(state => ({...state, gameName}));
    export const importScannedGame = () => globalThis.feedbackImport.add();
    export const browseGameDirectory = async () => {};
    export const chooseCandidate = () => {};
    export const pickExecutable = async () => {};
    export const rescanCurrentDirectory = async () => {};
  `);
  mocks["src/lib/services/manual-import"] = dataModule("export const previewManualGameSteamAppId = async () => ({ status: 'not_found', steamAppId: null, name: null });");
  mocks["src/lib/services/catalog"] = dataModule("export const cachedCatalogSearch = async () => ({games:[]}); export const refreshCatalogCached = async () => {}; export const searchCatalog = async () => ({games:[]});");
  mocks["src/lib/stores/steam-details"] = dataModule(storeImport + 'export const steamDetails = writable({}); export const ensureSteamDetails = () => {};');
  mocks["src/lib/features/library/SteamArtwork.svelte"] = dataModule("export default () => {};");
  let imports = 0;
  let closed = 0;
  globalThis.feedbackImport.add = async () => {
    imports++;
    games.set({ data: [game] });
    return { game, linkingError: null, shortcutWarning: "Could not extract shortcut icon: invalid icon definition" };
  };
  const component = (await import(await load("src/lib/features/library/AddGameDialog.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(component, { target, props: { onClose: () => closed++, prefill: { steamAppId: 400, name: "My game", suggestedName: "My game" } } });
  try {
    await settle();
    const add = [...target.querySelectorAll("button")].find(button => button.textContent.includes("Aggiungi alla libreria"));
    assert.equal(add.disabled, false);
    add.click();
    await settle();
    assert.equal(imports, 1);
    assert.equal(closed, 1);
    assert.ok(get(notices.toast).some(notice => notice.tone === "success"));
    assert.ok(get(notices.toast).some(notice => notice.message.includes("shortcut icon")));
    assert.ok(!target.textContent.includes("Could not extract shortcut icon"));
  } finally { await unmount(instance); target.remove(); clearNotices(); delete globalThis.feedbackImport; }
});

test("notifications remain dismissible while a modal is open and return outside after it closes", async () => {
  const popovers = new WeakSet();
  const query = document.querySelectorAll.bind(document);
  const matches = Element.prototype.matches;
  document.querySelectorAll = (selector) => query(selector === "dialog:modal" ? "dialog[open]" : selector);
  Element.prototype.matches = function (selector) { return selector === ":popover-open" ? popovers.has(this) : matches.call(this, selector); };
  HTMLElement.prototype.showPopover = function () { popovers.add(this); };
  HTMLElement.prototype.hidePopover = function () { popovers.delete(this); };
  const dialog = document.createElement("dialog");
  dialog.setAttribute("open", "");
  const target = document.createElement("div");
  document.body.append(dialog, target);
  const component = (await import(await load("src/lib/components/ui/ToastNotice.svelte"))).default;
  const instance = mount(component, { target });
  try {
    notices.showToast("Launch warning", "error");
    await settle();
    const notice = document.querySelector(".legio-toast");
    assert.equal(notice.parentNode, dialog);
    assert.ok(popovers.has(notice));
    notice.querySelector("button").click();
    await settle();
    assert.equal(get(notices.toast).length, 0);
    notices.showToast("Saved", "success");
    dialog.removeAttribute("open");
    await settle();
    assert.equal(notice.parentNode, target);
    assert.ok(popovers.has(notice));
    notice.querySelector("button").click();
    await settle();
    assert.equal(get(notices.toast).length, 0);
  } finally {
    await unmount(instance);
    clearNotices();
    dialog.remove(); target.remove();
    document.querySelectorAll = query;
    Element.prototype.matches = matches;
    delete HTMLElement.prototype.showPopover;
    delete HTMLElement.prototype.hidePopover;
  }
});
