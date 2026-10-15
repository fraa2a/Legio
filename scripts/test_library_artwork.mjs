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
  "@tauri-apps/api/core": dataModule('export const invoke = (...args) => args[0] === "cancel_artwork_request" ? (globalThis.cancelledMetadata?.push(args[1].requestId), Promise.resolve()) : globalThis.artworkInvoke(...args);'),
  "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language = writable("en"); export const t = value => value;`),
  "src/lib/utils/motion": dataModule("export const fadeDuration = 0;"),
};
const { load: moduleUrl, resolveImports } = createModuleLoader(mocks);
const logger = await import(await moduleUrl("src/lib/services/application-log.ts"));
logger.setApplicationLoggingEnabled(false);
const requests = [];
globalThis.artworkInvoke = (command, args) => {
  requests.push([command, args]);
  assert.equal(command, "get_steam_asset");
  return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null }));
};
const settle = async () => {
  for (let i = 0; i < 8; i++) { await new Promise((resolve) => setImmediate(resolve)); flushSync(); }
};

// Each App ID starts with empty details and image caches.
test("library cover releases its image outside the viewport and reuses the shared cache on return", async () => {
  const images = await import(await moduleUrl("src/lib/services/steam-details.ts"));
  await images.loadSteamImage({ steamAppId: 7999, asset: "hero_blur", fallbackAsset: null, index: null, version: null, full: false });
  requests.length = 0;
  let intersect;
  globalThis.IntersectionObserver = class { constructor(callback) { intersect = callback; } observe() {} disconnect() {} };
  const source = `<script>import ArtworkTile from "./src/lib/components/ui/ArtworkTile.svelte";</script><ul><ArtworkTile steamAppId={7999} monogram="P">Portal</ArtworkTile></ul>`;
  const url = dataModule(await resolveImports(compile(source, { generate: "client" }).js.code, "offscreen-fixture.svelte"));
  const target = document.createElement("div");
  const component = mount((await import(url)).default, { target });
  try {
    await settle(); intersect([{ isIntersecting: true }]); await settle();
    const before = target.querySelector("img").src;
    intersect([{ isIntersecting: false }]); await settle();
    assert.equal(target.querySelector("img"), null);
    intersect([{ isIntersecting: true }]); await settle();
    assert.equal(target.querySelector("img").src, before);
    assert.equal(requests.filter(([command]) => command === "get_steam_asset").length, 0);
  } finally { await unmount(component); delete globalThis.IntersectionObserver; }
});

test("visible library cover loads without requesting textual metadata", async () => {
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
    assert.equal(requests.filter(([command]) => command === "get_steam_details").length, 0);
    assert.ok(requests.some(([command, args]) => command === "get_steam_asset" && args.steamAppId === 400 && args.asset === "hero_blur"));
    assert.match(target.querySelector("img")?.src ?? "", /^blob:/);
    assert.equal(requests.find(([command]) => command === "get_steam_asset")[1].origin, "visible");
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
    if (args.policy === "refresh_if_expired") return new Promise(resolve => { completeRefresh = resolve; });
    return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/webp", stale: true, cacheWarning: null, refreshAfter: 0 }));
  };
  const target = document.createElement("div");
  const component = mount(SteamArtwork, { target, props: { steamAppId: 501, asset: "hero" } });
  try {
    await settle();
    const oldUrl = target.querySelector("img").src;
    const images = await import(await moduleUrl("src/lib/services/steam-details.ts"));
    const opening = images.refreshSteamImagesOnOpen(501, { requestId: "open-501", origin: "visible", release() {} });
    await settle();
    assert.equal(calls.length, 2);
    assert.equal(calls[1].policy, "refresh_if_expired");
    await settle();
    assert.equal(target.querySelector("img").src, oldUrl);
    completeRefresh(artworkPacket({ bytes: [2], contentType: "image/webp", stale: false, cacheWarning: null, refreshAfter: Date.now() + 120 * 3600000 }));
    await opening; await settle();
    assert.notEqual(target.querySelector("img").src, oldUrl);
    assert.equal(calls.length, 2);
  } finally { await unmount(component); }
});

test("failed background refresh preserves the cached cover", async () => {
  const { default: SteamArtwork } = await import(await moduleUrl("src/lib/features/library/SteamArtwork.svelte"));
  globalThis.artworkInvoke = (command, args) => args.policy === "refresh_if_expired"
    ? Promise.reject(new Error("offline"))
    : Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/webp", stale: true, cacheWarning: null }));
  const target = document.createElement("div");
  const component = mount(SteamArtwork, { target, props: { steamAppId: 502, asset: "hero" } });
  try {
    await settle();
    assert.match(target.querySelector("img").src, /^blob:/);
    const images = await import(await moduleUrl("src/lib/services/steam-details.ts"));
    await assert.rejects(images.refreshSteamImagesOnOpen(502, { requestId: "open-502", origin: "visible", release() {} }));
    await settle(); assert.match(target.textContent, /offline/);
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

test("library cards keep a manual game's name without requesting a small icon", async () => {
  const { default: GameCard } = await import(await moduleUrl("src/lib/features/library/GameCard.svelte"));
  globalThis.artworkInvoke = () => assert.fail("Manual cards must not request icon artwork");
  const target = document.createElement("div");
  const component = mount(GameCard, { target, props: {
    game: { id: "card-icon", name: "Manual game", steamAppId: null }, launch: undefined, onOpen() {},
  } });
  try {
    await settle();
    assert.equal(target.querySelector("img"), null);
    assert.match(target.textContent, /Manual game/);
  } finally { await unmount(component); }
});

test("library cards use the branding logo instead of a small icon", async () => {
  const { default: GameCard } = await import(await moduleUrl("src/lib/features/library/GameCard.svelte"));
  const calls = [];
  globalThis.artworkInvoke = (command, args) => {
    calls.push([command, args]);
    if (command === "get_steam_details") return Promise.resolve({ details: null, cachedAt: null, stale: false });
    assert.equal(command, "get_steam_asset");
    assert.ok(["logo", "hero_blur"].includes(args.asset));
    return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null }));
  };
  const target = document.createElement("div");
  const component = mount(GameCard, { target, props: {
    game: { id: "card-logo", name: "Portal", steamAppId: 7142 }, launch: undefined, onOpen() {},
  } });
  try {
    await settle();
    assert.equal(calls.filter(([command, args]) => command === "get_steam_asset" && args.asset === "logo").length, 1);
    assert.equal(target.querySelectorAll("img").length, 2);
  } finally { await unmount(component); }
});

test("library renders repeated Steam diagnostics and stays usable after refresh", async () => {
  const games = writable({ data: [], status: "ready", error: null });
  games.load = async () => {};
  const steamLibrary = writable({ importing: false, error: null, importResult: null });
  globalThis.libraryCrashFixture = { games, steamLibrary };
  const storeImport = `import { writable } from "${import.meta.resolve("svelte/store")}";`;
  const { load: libraryModuleUrl } = createModuleLoader({ ...mocks,
    "src/lib/stores/games": dataModule("export const games = globalThis.libraryCrashFixture.games;"),
    "src/lib/stores/steam-library": dataModule("export const steamLibrary = globalThis.libraryCrashFixture.steamLibrary; export const importSteamLibrary = async () => {};"),
    "src/lib/stores/settings": dataModule(storeImport + 'export const settings = writable({ data: { syncSteamGames: true } });'),
    "src/lib/stores/library-ui": dataModule(storeImport + 'export const addGameDialogOpen = writable(false), libraryPortrait = writable(false), libraryQuery = writable("");'),
    "src/lib/stores/launch": dataModule(storeImport + "export const launchError = writable(null), launchStateByGame = writable(new Map());"),
    "src/lib/stores/playtime": dataModule(storeImport + "export const playtime = writable({data:[]}); playtime.load = async () => {};"),
    "src/lib/stores/navigation": dataModule("export const openGame = () => {};"),
    "src/lib/features/library/GameCard.svelte": dataModule(await resolveImports(compile('<script>let { game } = $props();</script><li>{game.name}</li>', { generate: "client" }).js.code, "card-fixture.svelte")),
    "src/lib/features/library/AddGameDialog.svelte": dataModule("export default () => {};"),
  });
  const { default: LibraryView } = await import(await libraryModuleUrl("src/lib/features/library/LibraryView.svelte"));
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

test("Store row releases metadata demand as soon as it leaves view", async () => {
  let detailsResolve;
  globalThis.cancelledMetadata = [];
  requests.length = 0;
  globalThis.artworkInvoke = (command, args) => {
    requests.push([command, args]);
    if (command === "get_steam_details") return new Promise(resolve => { detailsResolve = resolve; });
    if (command === "get_steam_asset") return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null }));
    return Promise.resolve();
  };
  let intersect;
  globalThis.IntersectionObserver = class { constructor(callback) { intersect = callback; } observe() {} disconnect() {} };
  const { default: StoreGameCard } = await import(await moduleUrl("src/lib/features/store/StoreGameCard.svelte"));
  const target = document.createElement("ul"); document.body.append(target);
  const component = mount(StoreGameCard, { target, props: { steamAppId: 824, name: "Portal", status: { availability: "unavailable", entry: null }, onOpen() {} } });
  try {
    await settle(); intersect([{ isIntersecting: true }]); await settle();
    const metadata = requests.find(([command, args]) => command === "get_steam_details" && args.steamAppId === 824);
    assert.ok(metadata, "visible row starts metadata");
    intersect([{ isIntersecting: false }]); await settle();
    assert.ok(globalThis.cancelledMetadata.includes(metadata[1].requestId), "offscreen row must release native metadata demand");
    detailsResolve({ details: null, cachedAt: null, stale: false }); await settle();
  } finally {
    await unmount(component); target.remove(); delete globalThis.IntersectionObserver; delete globalThis.cancelledMetadata;
  }
});


test("offscreen library cards do not request branding logos", async () => {
  const { default: GameCard } = await import(await moduleUrl("src/lib/features/library/GameCard.svelte"));
  const calls = [];
  globalThis.IntersectionObserver = class { constructor(callback) { this.callback = callback; } observe() {} disconnect() {} };
  globalThis.artworkInvoke = (command, args) => { calls.push([command, args]); return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null })); };
  const target = document.createElement("div");
  const component = mount(GameCard, { target, props: { game: { id: "offscreen-logo", name: "Hidden game", steamAppId: 7900 }, launch: undefined, onOpen() {} } });
  try { await settle(); assert.equal(calls.filter(([command]) => command === "get_steam_asset").length, 0); }
  finally { await unmount(component); delete globalThis.IntersectionObserver; }
});

test("offscreen library cards release cached branding as well as their covers", async () => {
  const calls = [];
  globalThis.artworkInvoke = (command, args) => {
    calls.push([command, args]);
    return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null }));
  };
  const images = await import(await moduleUrl("src/lib/services/steam-details.ts"));
  const logo = await images.loadSteamImage({ steamAppId: 7998, asset: "logo", fallbackAsset: null, index: null, full: false });
  calls.length = 0;
  let intersect;
  globalThis.IntersectionObserver = class { constructor(callback) { intersect = callback; } observe() {} disconnect() {} };
  const { default: GameCard } = await import(await moduleUrl("src/lib/features/library/GameCard.svelte"));
  const target = document.createElement("div");
  const component = mount(GameCard, { target, props: { game: { id: "cached-branding", name: "Portal", steamAppId: 7998 }, onOpen() {} } });
  try {
    await settle();
    assert.equal(target.querySelectorAll("img").length, 0);
    intersect([{ isIntersecting: true }]); await settle();
    assert.ok([...target.querySelectorAll("img")].some(image => image.src === logo.url));
    intersect([{ isIntersecting: false }]); await settle();
    assert.equal(target.querySelectorAll("img").length, 0);
    intersect([{ isIntersecting: true }]); await settle();
    assert.ok([...target.querySelectorAll("img")].some(image => image.src === logo.url));
    assert.equal(calls.filter(([command, args]) => command === "get_steam_asset" && args.asset === "logo").length, 0);
  } finally { await unmount(component); delete globalThis.IntersectionObserver; }
});

test("outgoing page preserves its cached image after demand stops", async () => {
  globalThis.pageArtworkActive = writable(true);
  const calls = [];
  globalThis.artworkInvoke = (command, args) => {
    calls.push([command, args]);
    return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null }));
  };
  const source = `<script>import ArtworkScope from "./src/lib/components/ui/ArtworkScope.svelte"; import SteamArtwork from "./src/lib/features/library/SteamArtwork.svelte"; const active = globalThis.pageArtworkActive;</script><ArtworkScope active={$active}><SteamArtwork steamAppId={7901} asset="header" caption={false}/></ArtworkScope>`;
  const url = dataModule(await resolveImports(compile(source, { generate: "client" }).js.code, "fixture.svelte"));
  const target = document.createElement("div");
  const component = mount((await import(url)).default, { target });
  try {
    await settle(); const before = target.querySelector("img").src;
    globalThis.pageArtworkActive.set(false); await settle();
    assert.equal(target.querySelector("img").src, before);
    assert.equal(calls.filter(([command]) => command === "get_steam_asset").length, 1);
  } finally { await unmount(component); }
});


test("outgoing page cancels a pending image before unmount", async () => {
  globalThis.pageArtworkActive = writable(true);
  globalThis.cancelledMetadata = [];
  let complete;
  globalThis.artworkInvoke = () => new Promise(resolve => { complete = resolve; });
  const source = `<script>import ArtworkScope from "./src/lib/components/ui/ArtworkScope.svelte"; import SteamArtwork from "./src/lib/features/library/SteamArtwork.svelte"; const active = globalThis.pageArtworkActive;</script><ArtworkScope active={$active}><SteamArtwork steamAppId={7902} asset="header" caption={false}/></ArtworkScope>`;
  const url = dataModule(await resolveImports(compile(source, { generate: "client" }).js.code, "pending-fixture.svelte"));
  const component = mount((await import(url)).default, { target: document.createElement("div") });
  try {
    await settle(); assert.equal(typeof complete, "function");
    globalThis.pageArtworkActive.set(false); await settle();
    assert.equal(globalThis.cancelledMetadata.length, 1);
  } finally {
    complete(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null }));
    await settle(); await unmount(component); delete globalThis.cancelledMetadata;
  }
});


test("sidebar continues observing after its icon becomes visible", async () => {
  let intersect; let disconnects = 0;
  globalThis.IntersectionObserver = class { constructor(callback) { intersect = callback; } observe() {} disconnect() { disconnects++; } };
  const { default: Item } = await import(await moduleUrl("src/lib/components/layout/SidebarGameItem.svelte"));
  const component = mount(Item, { target: document.createElement("ul"), props: { game: { id: "observer", name: "Manual", steamAppId: null }, expanded: false } });
  try {
    await settle(); const initial = disconnects;
    intersect([{ isIntersecting: true }]); await settle();
    assert.equal(disconnects, initial, "Visibility changes must not disconnect the viewport observer");
    intersect([{ isIntersecting: false }]); await settle();
  } finally { await unmount(component); delete globalThis.IntersectionObserver; }
});
