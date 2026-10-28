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

test("progress variants share bounded accessible values and valid button markup", async () => {
  const ProgressBar = (await import(await load("src/lib/components/ui/ProgressBar.svelte"))).default;
  for (const variant of ["default", "button"]) {
    for (const [value, expected] of [[-10, 0], [140, 100], [NaN, 0], [Infinity, 0], [12.5, 12.5]]) {
      const target = document.createElement(variant === "button" ? "button" : "div");
      const instance = mount(ProgressBar, { target, props: { variant, value, label: "Download progress" } });
      try {
        await settle();
        const bar = target.querySelector('[role="progressbar"]');
        assert.equal(bar.getAttribute("aria-label"), "Download progress");
        assert.equal(bar.getAttribute("aria-valuenow"), String(Math.round(expected)));
        assert.equal(bar.firstElementChild.style.width, `${expected}%`);
        assert.equal(target.querySelector("div"), null);
      } finally { await unmount(instance); }
    }
  }
});

test("an error stays visible in its panel and retains its retry action", async () => {
  const ErrorBanner = (await import(await load("src/lib/components/ui/ErrorBanner.svelte"))).default;
  const target = document.createElement("div");
  let retried = 0;
  const instance = mount(ErrorBanner, { target, props: { message: "Failed to launch", onRetry: () => retried++ } });
  await settle();
  assert.match(target.querySelector('[role="alert"]')?.textContent ?? "", /Failed to launch/);
  assert.equal(get(notices.toast).some(notice => notice.message === "Failed to launch"), false);
  target.querySelector("button").click();
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

test("callouts expose their role, collapse long lists and offer retry and dismiss", async () => {
  const Callout = (await import(await load("src/lib/components/ui/Callout.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  let retried = 0; let dismissed = 0;
  const mountCallout = props => mount(Callout, { target, props });
  let instance = mountCallout({ tone: "danger", message: "Disk full", onRetry: () => { retried++; }, onDismiss: () => { dismissed++; } });
  try {
    await settle();
    assert.equal(target.firstElementChild.getAttribute("role"), "alert");
    assert.match(target.textContent, /Disk full/);
    [...target.querySelectorAll("button")].find(button => button.textContent.includes("Riprova")).click();
    target.querySelector('button[aria-label="Chiudi avviso"]').click();
    assert.deepEqual([retried, dismissed], [1, 1]);
    await unmount(instance); target.replaceChildren();
    instance = mountCallout({ tone: "warning", title: "Runner", items: ["one", "one"] });
    await settle();
    assert.equal(target.firstElementChild.getAttribute("role"), "status");
    assert.equal(target.querySelector("details"), null, "two items stay visible");
    assert.equal(target.querySelectorAll("li").length, 2);
    await unmount(instance); target.replaceChildren();
    instance = mountCallout({ tone: "warning", title: "Runner", items: ["a", "b", "b"] });
    await settle();
    assert.ok(target.querySelector("details"), "longer lists collapse");
    assert.match(target.querySelector("summary").textContent, /Mostra i dettagli \(3\)/);
    assert.equal(target.querySelector('ul[aria-label="Runner"]').children.length, 3);
  } finally { await unmount(instance); target.remove(); }
});

test("state blocks show skeleton rows while loading and an empty state with its icon", async () => {
  const StateBlock = (await import(await load("src/lib/components/ui/StateBlock.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  let instance = mount(StateBlock, { target, props: { status: "loading", error: null, skeletonRows: 3, loadingMessage: "Loading list" } });
  try {
    await settle();
    assert.equal(target.querySelectorAll("[aria-hidden='true'].animate-pulse").length, 3);
    assert.equal(target.querySelector('[role="status"] .sr-only').textContent, "Loading list");
    await unmount(instance); target.replaceChildren();
    instance = mount(StateBlock, { target, props: { status: "empty", error: null, emptyMessage: "Nothing here", emptyIcon: "library" } });
    await settle();
    assert.match(target.textContent, /Nothing here/);
    assert.ok(target.querySelector("svg, span.grid"), "an icon chip is rendered");
  } finally { await unmount(instance); target.remove(); }
});

test("icon button titles appear as a tooltip after a delay and hide on leave or Escape", async () => {
  const shown = [];
  HTMLElement.prototype.showPopover = function () { shown.push(this); };
  const { createRawSnippet } = await import("svelte");
  const IconButton = (await import(await load("src/lib/components/ui/IconButton.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(IconButton, { target, props: { label: "Reload", title: "Reload store", onClick() {}, children: createRawSnippet(() => ({ render: () => "<i>x</i>" })) } });
  try {
    await settle();
    const button = target.querySelector("button");
    assert.equal(button.getAttribute("title"), null, "the native title is replaced");
    assert.equal(button.getAttribute("aria-label"), "Reload");
    const anchor = button.parentElement;
    anchor.dispatchEvent(new window.Event("pointerenter"));
    await new Promise(resolve => setTimeout(resolve, 100)); await settle();
    assert.equal(target.querySelector("[popover]"), null, "nothing shows before the delay");
    await new Promise(resolve => setTimeout(resolve, 450)); await settle();
    assert.equal(target.querySelector("[popover]").textContent, "Reload store");
    assert.equal(shown.length, 1);
    anchor.dispatchEvent(new window.Event("pointerleave"));
    await settle();
    assert.equal(target.querySelector("[popover]"), null);
    button.dispatchEvent(new window.Event("focusin", { bubbles: true }));
    await new Promise(resolve => setTimeout(resolve, 20)); await settle();
    assert.ok(target.querySelector("[popover]"), "keyboard focus shows it immediately");
    window.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Escape" }));
    await settle();
    assert.equal(target.querySelector("[popover]"), null);
  } finally { await unmount(instance); target.remove(); }
});

test("command palette ranks games and pages, moves with the arrows and runs the chosen entry", async () => {
  const calls = [];
  globalThis.paletteFixture = { calls, games: writable({ data: [{ id: "g1", name: "Hollow Knight" }, { id: "g2", name: "Portal 2" }] }), query: writable("") };
  const { load: loadPalette } = createModuleLoader({
    ...mocks,
    "src/lib/stores/games": dataModule("export const games = globalThis.paletteFixture.games;"),
    "src/lib/stores/app-info": dataModule(storeImport + 'export const appInfo = writable({ data: { platform: "linux" } });'),
    "src/lib/stores/library-ui": dataModule("export const addGameDialogOpen = { set: value => globalThis.paletteFixture.calls.push(['add-dialog', value]) }; export const storeQuery = globalThis.paletteFixture.query;"),
    "src/lib/stores/navigation": dataModule(`export const sections = ["home", "library", "store", "downloads"]; export const sectionLabels = { home: "Home", library: "Libreria", store: "Store", downloads: "Download" };
      export const selectSection = section => globalThis.paletteFixture.calls.push(["section", section]);
      export const openGame = id => globalThis.paletteFixture.calls.push(["game", id]);
      export const openSettings = id => globalThis.paletteFixture.calls.push(["settings", id]);`),
  });
  const { paletteOpen } = await import(await loadPalette("src/lib/stores/command-palette.ts"));
  const CommandPalette = (await import(await loadPalette("src/lib/features/palette/CommandPalette.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  paletteOpen.set(true);
  const instance = mount(CommandPalette, { target });
  const press = (key) => document.activeElement.dispatchEvent(new window.KeyboardEvent("keydown", { key, bubbles: true }));
  const type = (value) => { const input = document.querySelector('input[role="combobox"]'); input.value = value; input.dispatchEvent(new window.Event("input", { bubbles: true })); };
  const labels = () => [...document.querySelectorAll('[role="option"]')].map(option => option.querySelector("span.truncate").textContent);
  try {
    await settle();
    assert.equal(document.activeElement, document.querySelector('input[role="combobox"]'));
    assert.deepEqual(labels().slice(0, 4), ["Home", "Libreria", "Store", "Download"]);
    type("kni"); await settle();
    assert.deepEqual(labels(), ["Hollow Knight", "Cerca nello Store: kni"]);
    press("ArrowDown"); await settle();
    assert.equal(document.querySelector('[aria-selected="true"]').textContent.includes("Cerca nello Store"), true);
    press("Enter"); await settle();
    assert.deepEqual(calls, [["section", "store"]]);
    assert.equal(get(globalThis.paletteFixture.query), "kni");
    assert.equal(get(paletteOpen), false, "choosing an entry closes the palette");
    calls.length = 0; paletteOpen.set(true); await settle();
    type("aspetto"); await settle();
    press("Enter"); await settle();
    assert.deepEqual(calls, [["settings", "appearance"]]);
    paletteOpen.set(true); await settle();
    type("zzzz"); await settle();
    assert.deepEqual(labels(), ["Cerca nello Store: zzzz"]);
  } finally { await unmount(instance); target.remove(); delete globalThis.paletteFixture; }
});
