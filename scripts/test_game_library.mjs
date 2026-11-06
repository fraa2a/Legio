import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLInputElement", "Element", "Node", "Text", "Comment", "Event", "MutationObserver"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
const { mount, unmount, flushSync } = await import("svelte");
const { writable } = await import("svelte/store");
globalThis.matchMedia = () => ({ matches: false });
dom.window.HTMLCanvasElement.prototype.getContext = () => null;
dom.window.HTMLMediaElement.prototype.pause = function () {};
const settle = async () => { for (let i = 0; i < 5; i++) { await new Promise(resolve => setImmediate(resolve)); flushSync(); } };

test("uninstall previews the exact directory and requires a second confirmation", async () => {
  const calls = [];
  globalThis.uninstallFixture = { calls };
  const { load } = createModuleLoader({
    "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language=writable("it"); export const t=value=>value;`),
    "src/lib/services/local-state": dataModule('export const getGameUninstallTarget=async id=>{globalThis.uninstallFixture.calls.push(["preview",id]);return "/games/My Game";};'),
    "src/lib/stores/games": dataModule('export const deleteGame=async()=>{}; export const hideSteamGame=async()=>{}; export const saveGame=async()=>{}; export const uninstallGame=async(...args)=>globalThis.uninstallFixture.calls.push(["uninstall",...args]);'),
    "src/lib/stores/settings": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const settings=writable({data:{syncSteamGames:true}});`),
    "src/lib/features/library/SteamGameSearch.svelte": dataModule(await createModuleLoader({}).resolveImports(compile('<div></div>', { generate: "client" }).js.code, "stub.svelte")),
    "src/lib/components/ui/Dialog.svelte": dataModule(await createModuleLoader({}).resolveImports(compile('<script>let {open,children,onClose}=$props();</script>{#if open}<section role="dialog">{@render children(onClose)}</section>{/if}', { generate: "client" }).js.code, "stub.svelte")),
  });
  const Component = (await import(await load("src/lib/features/library/GameDetailsPanel.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(Component, { target, props: { game: { id: "local", name: "My Game", steamAppId: 400, steamInstallPath: null, executablePath: "/games/My Game/game.exe", installationRoot: "/games/My Game", nameOverride: null, automaticName: "My Game" }, section: "danger", onRemoved: () => calls.push(["removed"]) } });
  try {
    await settle();
    assert.deepEqual(calls, []);
    const findButton = label => [...target.querySelectorAll("button")].find(button => button.textContent.trim() === label);
    findButton("Disinstalla gioco").click(); await settle();
    assert.deepEqual(calls, [["preview", "local"]]);
    assert.ok(target.textContent.includes("/games/My Game"));
    assert.ok(target.querySelector('[role="dialog"]'));
    findButton("Annulla").click(); await settle();
    assert.equal(target.querySelector('[role="dialog"]'), null);
    assert.deepEqual(calls, [["preview", "local"]], "cancel never uninstalls files");
    findButton("Disinstalla gioco").click(); await settle();
    const button = findButton("Disinstalla definitivamente");
    assert.ok(button);
    button.click(); await settle();
    assert.deepEqual(calls, [["preview", "local"], ["preview", "local"], ["uninstall", "local", "/games/My Game"], ["removed"]]);
  } finally { await unmount(instance); target.remove(); delete globalThis.uninstallFixture; }
});

test("transfer asks only for a destination library and stays disabled while the game runs", async () => {
  const launch = writable(new Map());
  const game = { id: "game", name: "My Game", steamInstallPath: null, executablePath: "/old/Game/Game.exe", installationRoot: "/old/Game" };
  const calls = [];
  globalThis.libraryFixture = { launch, calls, game };
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  const stub = async source => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
  const empty = await stub("<div></div>");
  const mocks = {
    "src/lib/i18n": dataModule(storeImport + 'export const language=writable("it"); export const t=value=>value;'),
    "src/lib/stores/app-info": dataModule(storeImport + 'export const appInfo=writable({data:{platform:"windows"}});'),
    "src/lib/stores/launch": dataModule('export const launchStateByGame=globalThis.libraryFixture.launch;'),
    "src/lib/stores/playtime": dataModule(storeImport + 'export const playtime=writable({data:[{gameId:"game",totalMilliseconds:3600000,activeSessions:0}]}); playtime.load=async()=>{}; playtime.reload=async()=>{};'),
    "src/lib/services/playtime": dataModule('export const setGamePlaytime=async(...args)=>{globalThis.libraryFixture.calls.push(args);};'),
    "src/lib/stores/games": dataModule('export const reconcileGame=()=>{};'),
    "src/lib/stores/game-libraries": dataModule(storeImport + 'export const gameLibraries=writable({data:[{id:"current",root:"/old",games:[{id:"game",name:"My Game"}]}]}); gameLibraries.load=async()=>{}; gameLibraries.reload=async()=>{};'),
    "src/lib/features/library/LibraryPickerDialog.svelte": await stub('<script>let {open,excludeId,onSelect}=$props();</script>{#if open}<button onclick={() => onSelect({id:"next",root:"/new-library"})}>Pick outside {excludeId}</button>{/if}'),
    "src/lib/services/local-state": dataModule('export const createDesktopShortcut=async()=>({}); export const transferGame=async(...args)=>{globalThis.libraryFixture.calls.push(args);return {game:globalThis.libraryFixture.game,warning:null};};'),
    "src/lib/components/ui/Dialog.svelte": await stub('<script>let {open,children}=$props();</script>{#if open}<div>{@render children?.()}</div>{/if}'),
  };
  for (const panel of ["GameArtworkPanel", "GameDetailsPanel", "ExecutablePanel", "GameSettingsPanel", "SteamAccountPanel", "SteamLaunchSettings"]) {
    mocks[`src/lib/features/library/${panel}.svelte`] = empty;
  }
  const loader = createModuleLoader(mocks);
  const Component = (await import(await loader.load("src/lib/features/library/GameSettingsDialog.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(Component, { target, props: { game, onClose() {}, onRemoved() {} } });
  try {
    await settle();
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Posizioni").click(); await settle();
    assert.equal(target.querySelector("#transfer-source"), null);
    const button = [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Sposta in un'altra libreria...");
    assert.equal(button.disabled, false);
    launch.set(new Map([[game.id, { status: "running" }]])); await settle();
    assert.equal(button.disabled, true);
    assert.equal(calls.length, 0);
    launch.set(new Map()); await settle();
    button.click(); await settle();
    assert.equal(calls.length, 0);
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Pick outside current").click(); await settle();
    assert.deepEqual(calls, [[game.id, "next"]]);
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Zona pericolosa").click(); await settle();
    const editPlaytime = [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Imposta tempo di gioco");
    assert.ok(editPlaytime, "the red playtime button stays in danger settings");
    assert.equal(target.querySelector("#playtime-hours"), null, "playtime fields stay in the popup");
    editPlaytime.click(); await settle();
    const hours = target.querySelector("#playtime-hours");
    assert.ok(hours, "the popup exposes editable hours and minutes");
    assert.ok(target.querySelector("#playtime-minutes"));
    assert.equal(hours.value, "1");
    hours.value = "2";
    hours.dispatchEvent(new dom.window.Event("input", {bubbles:true})); await settle();
    [...target.querySelectorAll("button")].find(button => button.textContent.trim() === "Conferma modifica").click(); await settle();
    assert.deepEqual(calls.at(-1), [game.id, 7200000]);
    assert.equal(target.querySelector("#playtime-hours"), null, "the popup closes after saving");
  } finally { await unmount(instance); target.remove(); delete globalThis.libraryFixture; }
});

test("Steam game settings offer hide and block removal while sync is enabled", async () => {
  const settings = writable({ data: { syncSteamGames: true } });
  const calls = [];
  globalThis.steamGameActions = { settings, calls };
  const stub = async source => dataModule(await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "stub.svelte"));
  const empty = await stub("<div></div>");
  const mocks = {
    "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language=writable("it"); export const t=value=>value;`),
    "src/lib/stores/settings": dataModule("export const settings = globalThis.steamGameActions.settings;"),
    "src/lib/stores/games": dataModule("export const uninstallGame=async()=>{}; export const deleteGame = async id => globalThis.steamGameActions.calls.push(['remove', id]); export const hideSteamGame = async id => globalThis.steamGameActions.calls.push(['hide', id]); export const saveGame = async () => {};"),
    "src/lib/features/library/SteamGameSearch.svelte": empty,
  };
  const { load } = createModuleLoader(mocks);
  const Panel = (await import(await load("src/lib/features/library/GameDetailsPanel.svelte"))).default;
  const game = { id: "steam-game", name: "Portal", steamAppId: 400, steamInstallPath: "/steam/Portal", executablePath: null, automaticName: "Portal", nameOverride: null };
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(Panel, { target, props: { game, section: "danger", onRemoved: () => calls.push(["closed"]) } });
  try {
    await settle();
    const button = label => [...target.querySelectorAll("button")].find(item => item.textContent.trim() === label);
    assert.equal(button("Disinstalla gioco"), undefined);
    assert.equal(button("Rimuovi gioco").disabled, true);
    assert.equal(button("Nascondi dalla libreria").disabled, false);
    settings.set({ data: { syncSteamGames: false } }); await settle();
    assert.equal(button("Rimuovi gioco").disabled, false);
    button("Nascondi dalla libreria").click(); await settle();
    assert.deepEqual(calls, [["hide", game.id], ["closed"]]);
  } finally { await unmount(instance); target.remove(); delete globalThis.steamGameActions; }
});

test("Steam descriptions keep link text without links or scripts", async () => {
  const { load } = createModuleLoader({});
  const { renderSteamDescription } = await import(await load("src/lib/features/library/steam-description.ts"));
  globalThis.DOMParser = dom.window.DOMParser;
  const target = document.createElement("div");
  renderSteamDescription(target, '<p>See <a href="https://example.com">the site</a><script>alert(1)</script></p>');
  assert.equal(target.innerHTML, "<p>See the site</p>");
  delete globalThis.DOMParser;
});

test("Steam description images and GIFs survive sanitization", async () => {
  const { load } = createModuleLoader({});
  const { renderSteamDescription } = await import(await load("src/lib/features/library/steam-description.ts"));
  globalThis.DOMParser = dom.window.DOMParser;
  const target = document.createElement("div");
  try {
    const gif = "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/400/demo.gif";
    const image = "https://images.steamusercontent.com/demo.png";
    const renderer = renderSteamDescription(target, `<p>Gameplay<img src="${gif}" alt="Gameplay preview" onerror="alert(1)" style="position:fixed" srcset="https://evil.test/image.png 2x"></p><a href="https://evil.test"><img src="${image}"></a>`);
    const images = [...target.querySelectorAll("img")];
    assert.equal(images.length, 2);
    assert.equal(images[0].src, gif);
    assert.equal(images[0].alt, "Gameplay preview");
    assert.equal(images[1].src, image);
    assert.equal(images[1].alt, "");
    assert.equal(images[0].getAttribute("loading"), "lazy");
    assert.equal(images[0].getAttribute("referrerpolicy"), "no-referrer");
    assert.equal(target.querySelector("[onerror],[style],[srcset],a"), null);
    renderer.update("<p>New description</p>");
    assert.equal(target.querySelector("img"), null);
    assert.equal(target.textContent, "New description");
  } finally { delete globalThis.DOMParser; }
});

test("Steam GIFs suspend decoding outside the viewport and resume near it", async () => {
  const { load } = createModuleLoader({});
  const { renderSteamDescription } = await import(await load("src/lib/features/library/steam-description.ts"));
  globalThis.DOMParser = dom.window.DOMParser;
  const observers = [];
  globalThis.IntersectionObserver = class {
    constructor(callback, options) { this.callback = callback; this.options = options; this.targets = new Set(); observers.push(this); }
    observe(target) { this.targets.add(target); }
    disconnect() { this.targets.clear(); this.disconnected = true; }
    intersect(target, isIntersecting) { this.callback([{ target, isIntersecting }]); }
  };
  const target = document.createElement("div");
  try {
    const renderer = renderSteamDescription(target, '<img src="https://shared.fastly.steamstatic.com/demo.gif"><img src="https://shared.fastly.steamstatic.com/demo.png">');
    const [gif, image] = target.querySelectorAll("img");
    assert.equal(observers.length, 1);
    assert.equal(observers[0].options.rootMargin, "300px 0px");
    assert.deepEqual([...observers[0].targets], [gif]);
    Object.defineProperties(gif, { complete: { configurable: true, value: true }, naturalWidth: { configurable: true, value: 640 }, naturalHeight: { configurable: true, value: 360 } });
    observers[0].intersect(gif, false);
    gif.dispatchEvent(new dom.window.Event("load"));
    assert.equal(gif.hasAttribute("src"), false, "offscreen GIFs release their animated source after loading");
    assert.equal(gif.getAttribute("width"), "640");
    assert.equal(gif.getAttribute("height"), "360");
    observers[0].intersect(gif, true);
    assert.equal(gif.src, "https://shared.fastly.steamstatic.com/demo.gif");
    assert.equal(image.src, "https://shared.fastly.steamstatic.com/demo.png", "static images stay loaded");
    renderer.destroy();
    assert.equal(observers[0].disconnected, true);
  } finally { delete globalThis.DOMParser; delete globalThis.IntersectionObserver; }
});

test("Steam descriptions reject untrusted image sources", async () => {
  const { load } = createModuleLoader({});
  const { renderSteamDescription } = await import(await load("src/lib/features/library/steam-description.ts"));
  globalThis.DOMParser = dom.window.DOMParser;
  const target = document.createElement("div");
  try {
    for (const src of ["http://cdn.steamstatic.com/a.gif", "https://steamstatic.com.evil.test/a.png", "https://evilsteamstatic.com/a.png", "https://user@steamstatic.com/a.png", "https://steamstatic.com:444/a.png", "https://evil.test/a.png", "file:///tmp/a.png", "data:image/svg+xml,test", "javascript:alert(1)", "//cdn.steamstatic.com/a.png", "/image.png", ""]) {
      renderSteamDescription(target, `<p>Safe text<img src="${src}"></p>`);
      assert.equal(target.querySelector("img"), null, src);
      assert.equal(target.textContent, "Safe text");
    }
  } finally { delete globalThis.DOMParser; }
});

test("Steam descriptions retain PEAK video animations and codec fallbacks", async () => {
  const { load } = createModuleLoader({});
  const { renderSteamDescription } = await import(await load("src/lib/features/library/steam-description.ts"));
  globalThis.DOMParser = dom.window.DOMParser;
  const target = document.createElement("div");
  try {
    const base = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/3527290/extras/e82d11d9221b2a65adeb68d8cdbc3bd2";
    renderSteamDescription(target, `<video autoplay muted loop playsinline poster="${base}.poster.avif" width=600 height=170 onplay="alert(1)" style="position:fixed"><source src="${base}.webm" type="video/webm; codecs=vp9"><source src="${base}.mp4" type="video/mp4"></video>`);
    const video = target.querySelector("video");
    assert.ok(video, "Steam now delivers GIF-like animations as video elements");
    assert.equal(video.autoplay, true);
    assert.equal(video.muted, true);
    assert.equal(video.defaultMuted, true);
    assert.equal(video.loop, true);
    assert.equal(video.playsInline, true);
    assert.equal(video.controls, false);
    assert.equal(video.poster, `${base}.poster.avif`);
    assert.equal(video.width, 600);
    assert.equal(video.height, 170);
    assert.deepEqual([...video.querySelectorAll("source")].map(source => [source.src, source.type]), [[`${base}.webm`, "video/webm; codecs=vp9"], [`${base}.mp4`, "video/mp4"]]);
    assert.equal(target.querySelector("[onplay],[style]"), null);
  } finally { delete globalThis.DOMParser; }
});

test("Steam video animations validate every source and poster", async () => {
  const { load } = createModuleLoader({});
  const { renderSteamDescription } = await import(await load("src/lib/features/library/steam-description.ts"));
  globalThis.DOMParser = dom.window.DOMParser;
  const target = document.createElement("div");
  try {
    const safe = "https://shared.fastly.steamstatic.com/animation.webm";
    renderSteamDescription(target, `<video src="https://evil.test/a.mp4" poster="file:///tmp/a.png"><source src="https://steamstatic.com.evil.test/a.webm"><source src="${safe}" type="video/webm" onerror="alert(1)"></video>`);
    const video = target.querySelector("video");
    assert.ok(video);
    assert.equal(video.hasAttribute("src"), false);
    assert.equal(video.hasAttribute("poster"), false);
    assert.equal(video.querySelectorAll("source").length, 1);
    assert.equal(video.querySelector("source").src, safe);
    assert.equal(target.querySelector("[onerror]"), null);
    renderSteamDescription(target, '<video src="https://evil.test/a.mp4"><source src="javascript:alert(1)"></video><p>Safe text</p>');
    assert.equal(target.querySelector("video,source"), null);
    assert.equal(target.textContent, "Safe text");
    renderSteamDescription(target, `<video src="${safe}"></video>`);
    assert.equal(target.querySelector("video").src, safe);
  } finally { delete globalThis.DOMParser; }
});

test("Steam video animations respect reduced motion", async () => {
  const { load } = createModuleLoader({});
  const { renderSteamDescription } = await import(await load("src/lib/features/library/steam-description.ts"));
  globalThis.DOMParser = dom.window.DOMParser;
  const previousMatchMedia = globalThis.matchMedia;
  globalThis.matchMedia = () => ({ matches: true });
  try {
    const target = document.createElement("div");
    renderSteamDescription(target, '<video autoplay src="https://shared.fastly.steamstatic.com/animation.mp4"></video>');
    const video = target.querySelector("video");
    assert.ok(video);
    assert.equal(video.autoplay, false);
    assert.equal(video.controls, false);
  } finally { globalThis.matchMedia = previousMatchMedia; delete globalThis.DOMParser; }
});

test("library removal stops at the first failed game and keeps the library", async () => {
  const calls = [];
  globalThis.removalFixture = { calls, fail: "b", libraries: [] };
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  const { load } = createModuleLoader({
    "src/lib/i18n": dataModule(storeImport + 'export const language=writable("it"); export const t=value=>value;'),
    "src/lib/services/dialog": dataModule("export const pickGameDirectory=async()=>null;"),
    "src/lib/services/game-libraries": dataModule('export const listGameLibraries=async()=>globalThis.removalFixture.libraries; export const addGameLibrary=async()=>""; export const removeGameLibrary=async id=>{globalThis.removalFixture.calls.push(["remove",id]);};'),
    "src/lib/services/local-state": dataModule('export const getGameUninstallTarget=async()=>""; export const transferGame=async(id,library)=>{globalThis.removalFixture.calls.push(["move",id,library]); if(id===globalThis.removalFixture.fail) throw new Error("disk full"); return {game:{id},warning:null};};'),
    "src/lib/stores/games": dataModule("export const games={reload:async()=>{}}; export const reconcileGame=()=>{}; export const uninstallGame=async()=>({warnings:[]});"),
  });
  const { removeLibrary, onlyUsableLibrary } = await import(await load("src/lib/stores/game-libraries.ts"));
  try {
    const library = { id: "old", root: "/old", games: [{ id: "a", name: "A" }, { id: "b", name: "B" }, { id: "c", name: "C" }] };
    const removal = { kind: "move", target: { id: "new", root: "/new" } };
    const outcome = await removeLibrary(library, removal, () => {});
    assert.deepEqual(outcome.failure, { game: "B", message: "disk full" });
    assert.deepEqual(calls, [["move", "a", "new"], ["move", "b", "new"]]);
    globalThis.removalFixture.fail = null;
    calls.length = 0;
    assert.equal((await removeLibrary({ ...library, games: [{ id: "c", name: "C" }] }, removal, () => {})).failure, null);
    assert.deepEqual(calls, [["move", "c", "new"], ["remove", "old"]]);

    // The picker is skipped only for a single reachable library with enough room.
    const only = { id: "only", root: "/games", freeBytes: 500, games: [] };
    globalThis.removalFixture.libraries = [only];
    assert.equal(await onlyUsableLibrary(100), only);
    assert.equal(await onlyUsableLibrary(1000), null);
    globalThis.removalFixture.libraries = [only, { ...only, id: "other" }];
    assert.equal(await onlyUsableLibrary(null), null);
  } finally { delete globalThis.removalFixture; }
});
