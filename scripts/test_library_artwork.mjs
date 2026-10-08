import assert from "node:assert/strict";
import { test } from "node:test";
import { compile } from "svelte/compiler";
import { JSDOM } from "jsdom";
import { artworkPacket, createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "Node", "Text", "Comment", "Element", "HTMLElement", "HTMLMediaElement", "Event", "CustomEvent", "getComputedStyle", "requestAnimationFrame", "cancelAnimationFrame"]) {
  const value = dom.window[key];
  Object.defineProperty(globalThis, key, { configurable: true, value: typeof value === "function" && key.endsWith("AnimationFrame") ? value.bind(dom.window) : value });
}
const { mount, unmount, flushSync } = await import("svelte");
const { writable } = await import("svelte/store");
const mocks = {
  "src/lib/stores/window-activity": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const windowActive = writable(true);`),
  "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.artworkInvoke(...args);"),
  "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language = writable("en"); export const t = value => value;`),
  "src/lib/utils/motion": dataModule("export const fadeDuration = 0;"),
};
const { load: moduleUrl, resolveImports } = createModuleLoader(mocks);
const requests = [];
let detailsResolve;
globalThis.artworkInvoke = (command, args) => {
  requests.push([command, args]);
  if (command === "get_steam_details") return new Promise((resolve) => { detailsResolve = resolve; });
  assert.equal(command, "get_steam_asset");
  return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null }));
};
const settle = async () => {
  for (let i = 0; i < 8; i++) { await new Promise((resolve) => setImmediate(resolve)); flushSync(); }
};

// Each App ID starts with empty details and image caches.
test("visible library cover loads while details are pending and remains after an empty response", async () => {
  requests.length = 0;
  let intersect;
  globalThis.IntersectionObserver = class {
    constructor(callback) { intersect = callback; }
    observe() {}
    disconnect() {}
  };
  const source = `<script>import ArtworkTile from "./src/lib/components/ui/ArtworkTile.svelte";</script><ul><ArtworkTile steamAppId={400} monogram="P">Portal</ArtworkTile></ul>`;
  const url = dataModule(await resolveImports(compile(source, { generate: "client" }).js.code, "fixture.svelte"));
  const target = document.createElement("div");
  document.body.append(target);
  const component = mount((await import(url)).default, { target });
  try {
    await settle();
    assert.equal(requests.length, 0, "offscreen covers must remain lazy");
    intersect([{ isIntersecting: true }]);
    await settle();
    assert.ok(requests.some(([command]) => command === "get_steam_details"));
    assert.ok(requests.some(([command, args]) => command === "get_steam_asset" && args.steamAppId === 400 && args.asset === "hero_blur"));
    assert.match(target.querySelector("img")?.src ?? "", /^blob:/);
    detailsResolve({ details: null, cachedAt: null, stale: false });
    await settle();
    assert.match(target.querySelector("img")?.src ?? "", /^blob:/);
  } finally {
    await unmount(component);
    target.remove();
    delete globalThis.IntersectionObserver;
  }
});

test("standalone game logo loads without opening game details", async () => {
  requests.length = 0;
  const { default: GameLogo } = await import(await moduleUrl("src/lib/features/library/GameLogo.svelte"));
  const target = document.createElement("div");
  document.body.append(target);
  const component = mount(GameLogo, { target, props: { game: { id: "portal", name: "Portal", steamAppId: 401 } } });
  try {
    await settle();
    assert.ok(requests.some(([command, args]) => command === "get_steam_asset" && args.steamAppId === 401 && args.asset === "logo"));
    assert.match(target.querySelector("img")?.src ?? "", /^blob:/);
    assert.equal(requests.filter(([command]) => command === "get_steam_details").length, 0);
  } finally {
    await unmount(component);
    target.remove();
  }
});

test("manual game keeps its name without requesting Steam artwork", async () => {
  requests.length = 0;
  const { default: GameLogo } = await import(await moduleUrl("src/lib/features/library/GameLogo.svelte"));
  const target = document.createElement("div");
  const component = mount(GameLogo, { target, props: { game: { id: "manual", name: "Manual game", steamAppId: null } } });
  try {
    await settle();
    assert.equal(target.textContent, "Manual game");
    assert.equal(requests.length, 0);
  } finally {
    await unmount(component);
  }
});

test("stale artwork stays visible during refresh and is replaced when ready", async () => {
  const { default: SteamArtwork } = await import(await moduleUrl("src/lib/features/library/SteamArtwork.svelte"));
  const calls = [];
  let completeRefresh;
  globalThis.artworkInvoke = (command, args) => {
    calls.push(args);
    if (args.refresh) return new Promise(resolve => { completeRefresh = resolve; });
    return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/webp", stale: true, cacheWarning: null, refreshAfter: 0 }));
  };
  const target = document.createElement("div");
  const component = mount(SteamArtwork, { target, props: { steamAppId: 501, asset: "hero" } });
  try {
    await settle();
    const oldUrl = target.querySelector("img").src;
    assert.equal(calls.length, 2);
    assert.equal(calls[1].refresh, true);
    await settle();
    assert.equal(target.querySelector("img").src, oldUrl);
    completeRefresh(artworkPacket({ bytes: [2], contentType: "image/webp", stale: false, cacheWarning: null, refreshAfter: Date.now() + 72 * 3600000 }));
    await settle();
    assert.notEqual(target.querySelector("img").src, oldUrl);
    assert.equal(calls.length, 2);
  } finally { await unmount(component); }
});

test("failed background refresh preserves the cached cover", async () => {
  const { default: SteamArtwork } = await import(await moduleUrl("src/lib/features/library/SteamArtwork.svelte"));
  globalThis.artworkInvoke = (command, args) => args.refresh
    ? Promise.reject(new Error("offline"))
    : Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/webp", stale: true, cacheWarning: null }));
  const target = document.createElement("div");
  const component = mount(SteamArtwork, { target, props: { steamAppId: 502, asset: "hero" } });
  try {
    await settle();
    assert.match(target.querySelector("img").src, /^blob:/);
    assert.match(target.textContent, /offline/);
  } finally { await unmount(component); }
});

test("game icons prioritize custom artwork, fall back to Steam client icons and reset without EXE extraction", async () => {
  const { default: GameIcon } = await import(await moduleUrl("src/lib/features/library/GameIcon.svelte"));
  const artwork = await import(await moduleUrl("src/lib/services/game-artwork.ts"));
  const game = { id: "custom-icon", name: "Portal", steamAppId: 7140, executablePath: "/games/game.exe" };
  let custom = true;
  let resolveCustom;
  requests.length = 0;
  globalThis.artworkInvoke = (command, args) => {
    requests.push([command, args]);
    if (command === "get_game_icon") return new Promise(resolve => { resolveCustom = resolve; });
    if (command === "reset_game_icon") { custom = false; return Promise.resolve(); }
    assert.equal(command, "get_steam_asset");
    assert.equal(args.asset, "client_icon");
    return Promise.resolve(artworkPacket({ bytes: [2], contentType: "image/png", stale: false, cacheWarning: null }));
  };
  const target = document.createElement("div");
  const component = mount(GameIcon, { target, props: { game } });
  try {
    await settle();
    assert.deepEqual(requests.map(([command]) => command), ["get_game_icon"], "wait for custom artwork before requesting a fallback");
    resolveCustom(artworkPacket({ bytes: [1], contentType: "image/png" }));
    await settle();
    const customUrl = target.querySelector("img").src;
    assert.match(customUrl, /^blob:/);
    assert.equal(target.querySelector("img").draggable, false);
    assert.equal(requests.length, 1);
    await artwork.resetGameIcon(game.id);
    await settle();
    assert.equal(custom, false);
    resolveCustom(new ArrayBuffer(0));
    await settle();
    assert.ok(requests.some(([command, args]) => command === "get_steam_asset" && args.asset === "client_icon"));
    assert.notEqual(target.querySelector("img").src, customUrl);
  } finally { await unmount(component); }
});

test("missing Steam client icons keep a monogram without substituting a branding logo", async () => {
  const { default: GameIcon } = await import(await moduleUrl("src/lib/features/library/GameIcon.svelte"));
  requests.length = 0;
  globalThis.artworkInvoke = (command, args) => {
    requests.push([command, args]);
    if (command === "get_game_icon") return Promise.resolve(new ArrayBuffer(0));
    assert.equal(command, "get_steam_asset");
    assert.equal(args.asset, "client_icon");
    return Promise.reject(new Error("No client icon"));
  };
  const target = document.createElement("div");
  const component = mount(GameIcon, { target, props: { game: { id: "missing-icon", name: "Portal", steamAppId: 7141 } } });
  try {
    await settle();
    assert.equal(target.querySelector("img"), null);
    assert.equal(target.textContent.trim(), "P");
    assert.equal(requests.filter(([command]) => command === "get_steam_asset").length, 1);
  } finally { await unmount(component); }
});

test("library cards show the small custom icon beside the game name", async () => {
  const { default: GameCard } = await import(await moduleUrl("src/lib/features/library/GameCard.svelte"));
  globalThis.artworkInvoke = (command) => {
    assert.equal(command, "get_game_icon");
    return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png" }));
  };
  const target = document.createElement("div");
  const component = mount(GameCard, { target, props: {
    game: { id: "card-icon", name: "Manual game", steamAppId: null }, launch: undefined, onOpen() {},
  } });
  try {
    await settle();
    const icon = target.querySelector('img[draggable="false"]');
    assert.ok(icon);
    assert.equal(icon.nextElementSibling.textContent, "Manual game");
  } finally { await unmount(component); }
});

test("library renders repeated Steam diagnostics and stays usable after refresh", async () => {
  const games = writable({ data: [], status: "ready", error: null });
  games.load = async () => {};
  const steamLibrary = writable({ importing: false, error: null, importResult: null });
  globalThis.libraryCrashFixture = { games, steamLibrary };
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  Object.assign(mocks, {
    "src/lib/stores/games": dataModule("export const games = globalThis.libraryCrashFixture.games;"),
    "src/lib/stores/steam-library": dataModule("export const steamLibrary = globalThis.libraryCrashFixture.steamLibrary; export const importSteamLibrary = async () => {};"),
    "src/lib/stores/library-ui": dataModule(storeImport + 'export const addGameDialogOpen = writable(false), libraryPortrait = writable(false), libraryQuery = writable("");'),
    "src/lib/stores/launch": dataModule(storeImport + "export const launchError = writable(null), launchStateByGame = writable(new Map());"),
    "src/lib/stores/playtime": dataModule(storeImport + "export const playtime = writable({data:[]}); playtime.load = async () => {};"),
    "src/lib/stores/navigation": dataModule("export const openGame = () => {};"),
    "src/lib/features/library/GameCard.svelte": dataModule(await resolveImports(compile('<script>let { game } = $props();</script><li>{game.name}</li>', { generate: "client" }).js.code, "card-fixture.svelte")),
    "src/lib/features/library/AddGameDialog.svelte": dataModule("export default () => {};"),
  });
  const { default: LibraryView } = await import(await moduleUrl("src/lib/features/library/LibraryView.svelte"));
  const target = document.createElement("div");
  document.body.append(target);
  let component;
  try {
    const warning = "ignored a Steam app without verified game type";
    steamLibrary.set({ importing: false, error: null, importResult: { diagnostics: [warning, warning] } });
    component = mount(LibraryView, { target });
    await settle();
    assert.equal(target.querySelectorAll('[aria-label="Diagnostica Steam"] li').length, 2);
    games.set({ data: [{ id: "portal", name: "Portal", steamAppId: 400, steamInstallPath: "C:\\Steam\\Portal" }], status: "ready", error: null });
    steamLibrary.set({ importing: false, error: null, importResult: { diagnostics: [warning, warning, warning] } });
    await settle();
    assert.equal(target.querySelectorAll('[aria-label="Diagnostica Steam"] li').length, 3);
    assert.match(target.textContent, /Portal/);
    steamLibrary.set({ importing: false, error: null, importResult: { diagnostics: [] } });
    await settle();
    assert.equal(target.querySelector('[aria-label="Diagnostica Steam"]'), null);
    assert.match(target.textContent, /Portal/);
  } finally {
    if (component) await unmount(component);
    target.remove();
  }
});
