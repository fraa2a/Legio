import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
import { get, writable } from "svelte/store";
import { artworkPacket, createModuleLoader } from "./test_module_loader.mjs";

const dataModule = (code) => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
const localeSource = await readFile(new URL("../src/lib/i18n/index.ts", import.meta.url), "utf8");
const catalog = await readFile(new URL("../src/lib/i18n/en.json", import.meta.url), "utf8");
const localeCode = ts.transpileModule(localeSource, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText
  .replace('from "svelte/store"', `from "${import.meta.resolve("svelte/store")}"`)
  .replace('from "./en.json"', `from "${dataModule("export default " + catalog)}"`);
const localeUrl = dataModule(localeCode);
async function loadModule(path, imports = {}) {
  const source = await readFile(new URL(path, import.meta.url), "utf8");
  let code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  for (const [specifier, url] of Object.entries({ "svelte/store": import.meta.resolve("svelte/store"), "../i18n": localeUrl, ...imports })) {
    code = code.replaceAll(`from "${specifier}"`, `from "${url}"`);
  }
  return import(dataModule(code));
}
const errors = dataModule("export const toMessage = error => String(error);");
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

test("resource deduplicates slow reads and invalidates responses after set", async () => {
  const { createResource } = await loadModule("../src/lib/stores/resource.ts", { "../utils/errors": errors });
  const first = deferred();
  let calls = 0;
  const resource = createResource([], () => { calls++; return first.promise; });
  const a = resource.load();
  const b = resource.load();
  assert.equal(calls, 1);
  first.resolve([1]);
  await Promise.all([a, b]);
  assert.equal(get(resource).status, "ready");
  assert.deepEqual(get(resource).data, [1]);
  const late = deferred();
  const other = createResource([], () => late.promise);
  const loading = other.load();
  other.set([2]);
  late.resolve([0]);
  await loading;
  assert.deepEqual(get(other).data, [2]);
});

test("Steam refresh survives older success and failure without losing cached data", async () => {
  const pending = [];
  globalThis.legioDetailsMock = () => { const request = deferred(); pending.push(request); return request.promise; };
  const service = dataModule("export const getSteamDetails = (...args) => globalThis.legioDetailsMock(...args);");
  const { steamDetails, loadSteamDetails } = await loadModule("../src/lib/stores/steam-details.ts", {
    "../services/steam-details": service, "../utils/errors": errors,
  });
  const old = loadSteamDetails(42, false);
  const fresh = loadSteamDetails(42, true);
  assert.equal(loadSteamDetails(42, false), fresh);
  pending[1].resolve({ details: { name: "NEW" }, cachedAt: 2, stale: false });
  await fresh;
  pending[0].resolve({ details: { name: "OLD" }, cachedAt: 1, stale: true });
  await old;
  assert.equal(get(steamDetails)[42].details.name, "NEW");
  const refresh = loadSteamDetails(42, true);
  pending[2].reject(new Error("offline"));
  await assert.rejects(refresh);
  assert.equal(get(steamDetails)[42].details.name, "NEW");
  assert.equal(get(steamDetails)[42].status, "ready");
  const { setLanguage } = await import(localeUrl);
  const oldLanguage = loadSteamDetails(43, false);
  setLanguage("it");
  const newLanguage = loadSteamDetails(43, false);
  pending[4].resolve({ details: { name: "ITALIAN" }, cachedAt: 4, stale: false });
  await newLanguage;
  pending[3].resolve({ details: { name: "ENGLISH" }, cachedAt: 3, stale: false });
  await oldLanguage;
  assert.equal(get(steamDetails)[43].details.name, "ITALIAN");
  setLanguage("en");
});

test("custom artwork shares URLs and invalidates only the changed kind", async () => {
  const calls = [];
  globalThis.legioInvokeMock = (command, args) => {
    calls.push([command, args]);
    return Promise.resolve(artworkPacket({ bytes: [1, 2], contentType: "image/png" }));
  };
  const api = dataModule("export const invoke = (...args) => globalThis.legioInvokeMock(...args);");
  const { load } = createModuleLoader({ "@tauri-apps/api/core": api });
  const { acquireGameArtwork, setGameIcon } = await import(await load("src/lib/services/game-artwork.ts"));
  const icon = acquireGameArtwork("game", "icon");
  const again = acquireGameArtwork("game", "icon");
  const banner = acquireGameArtwork("game", "banner");
  assert.equal(await icon.ready, await again.ready);
  const bannerUrl = await banner.ready;
  icon.release(); again.release(); banner.release();
  const cached = acquireGameArtwork("game", "icon");
  assert.equal(cached.url, await cached.ready);
  assert.equal(calls.length, 2);
  await setGameIcon("game", "/image.png");
  const keptBanner = acquireGameArtwork("game", "banner");
  assert.equal(keptBanner.url, bannerUrl);
  const changed = acquireGameArtwork("game", "icon");
  assert.notEqual(await changed.ready, cached.url);
  cached.release(); keptBanner.release(); changed.release();
});

test("language selection preserves regional formatting and interpolates translations", async () => {
  const { resolveLanguage, t } = await import(localeUrl);
  assert.equal(resolveLanguage(["de-DE", "it-CH", "en-US"]), "it");
  assert.equal(resolveLanguage(["fr-FR"]), "en");
  assert.equal(t("Versione {0}", "en", ["1.2"]), "Version 1.2");
  assert.equal(t("Versione {0}", "it", ["1.2"]), "Versione 1.2");
  assert.equal(t("  Chiudi  ", "en"), "  Close  ");
});

test("Steam image cache shares library artwork across details versions and retains versions for details assets", async () => {
  let requests = 0;
  globalThis.legioInvokeMock = () => {
    requests++;
    return Promise.resolve(artworkPacket({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null, refreshAfter: Date.now() + 60_000 }));
  };
  const api = dataModule("export const invoke = (...args) => globalThis.legioInvokeMock(...args);");
  const { load } = createModuleLoader({ "@tauri-apps/api/core": api, "src/lib/i18n": localeUrl });
  const { loadSteamImage } = await import(await load("src/lib/services/steam-details.ts"));
  const request = { steamAppId: 999, asset: "hero", fallbackAsset: null, index: null, version: 1, full: false };
  const oldImage = await loadSteamImage(request);
  const newImage = await loadSteamImage({ ...request, version: 2 });
  assert.equal(requests, 1);
  assert.equal(oldImage.url, newImage.url);
  const firstHeader = await loadSteamImage({ ...request, asset: "header" });
  const secondHeader = await loadSteamImage({ ...request, asset: "header", version: 2 });
  assert.equal(requests, 3);
  assert.notEqual(firstHeader.url, secondHeader.url);
  const hdRequest = { ...request, displayWidth: 1920 };
  const hd = await loadSteamImage(hdRequest);
  const full = await loadSteamImage({ ...hdRequest, full: true });
  assert.equal(hd.url, full.url, "viewer and banner share the selected monitor profile");
  assert.equal(requests, 3);
  const qhd = await loadSteamImage({ ...request, displayWidth: 2560 });
  assert.equal(requests, 4);
  assert.notEqual(hd.url, qhd.url);
  assert.equal((await loadSteamImage(hdRequest)).url, hd.url, "returning to a monitor profile reuses its cache");
  const unchangedHeader = await loadSteamImage({ ...request, asset: "header", displayWidth: 2560 });
  assert.equal(unchangedHeader.url, firstHeader.url);
  assert.equal(requests, 4, "monitor resolution only separates hero variants");
  URL.revokeObjectURL(qhd.url);
  URL.revokeObjectURL(oldImage.url);
  URL.revokeObjectURL(newImage.url);
});

test("binary artwork response retains metadata and views bytes without a second buffer", async () => {
  const { decodeImageResponse } = await loadModule("../src/lib/services/image-response.ts");
  const packet = artworkPacket({ bytes: [4, 5, 6], contentType: "image/webp", stale: true, cacheWarning: "offline", refreshAfter: 123 });
  const decoded = decodeImageResponse(packet);
  assert.equal(decoded.bytes.buffer, packet);
  assert.deepEqual([...decoded.bytes], [4, 5, 6]);
  assert.equal(decoded.cacheWarning, "offline");
  assert.equal(decoded.refreshAfter, 123);
  assert.throws(() => decodeImageResponse(new ArrayBuffer(2)));
});

test("catalog load more requests the next remote page", async () => {
  const pages = [];
  const game = { steamAppId: 400, name: "Portal", availability: "unavailable" };
  const result = (games, nextOffset, total = games.length) => ({ games, total, nextOffset, cachedAt: 1, stale: false, sourceCachedAt: null, sourceStale: false });
  globalThis.legioCatalogMock = {
    cachedCatalogSearch: () => null,
    searchCatalog: async (_query, limit) => result(limit ? [game, { ...game, steamAppId: 401 }] : [game], null),
    refreshCatalogCached: async () => result([game], 50, 2),
    refreshCatalogPage: async (_query, skip) => { pages.push(skip); return result([game, { ...game, steamAppId: 401 }, { ...game, steamAppId: 402 }], null); },
    rememberCatalogSearch: () => {},
  };
  const service = dataModule("export const cachedCatalogSearch = (...args) => globalThis.legioCatalogMock.cachedCatalogSearch(...args); export const searchCatalog = (...args) => globalThis.legioCatalogMock.searchCatalog(...args); export const refreshCatalogCached = (...args) => globalThis.legioCatalogMock.refreshCatalogCached(...args); export const refreshCatalogPage = (...args) => globalThis.legioCatalogMock.refreshCatalogPage(...args); export const rememberCatalogSearch = (...args) => globalThis.legioCatalogMock.rememberCatalogSearch(...args);");
  globalThis.legioSourceFixture = writable({ data: { manifest: null, stale: false } });
  const { load } = createModuleLoader({
    "src/lib/services/catalog": service,
    "src/lib/utils/errors": errors,
    "src/lib/stores/source": dataModule("export const source = globalThis.legioSourceFixture;"),
  });
  const { catalog: state, runCatalogSearch, loadMoreCatalog } = await import(await load("src/lib/stores/catalog.ts"));
  await runCatalogSearch("portal");
  assert.equal(get(state).nextOffset, 50);
  await loadMoreCatalog("portal", 40);
  assert.equal(get(state).nextOffset, 50);
  await loadMoreCatalog("portal", 60);
  assert.deepEqual(pages, [50]);
  assert.equal(get(state).results.length, 3);
  globalThis.legioSourceFixture.set({ data: { manifest: { verified: [{ steamAppId: 400 }], unverified: [] }, stale: false } });
  assert.equal(get(state).results[0].availability, "verified", "a refreshed source updates visible search results");
  globalThis.legioSourceFixture.set({ data: { manifest: null, stale: false } });
  assert.equal(get(state).results[0].availability, "unknown", "removing the last source clears cached availability");
});

test("library groups preserve matching versions and sum all installations through the summary index", async () => {
  const { libraryGroups, libraryItems } = await loadModule("../src/lib/features/library/library-model.ts");
  const games = [
    { id: "first", name: "Portal original", steamAppId: 400, steamInstallPath: "/steam" },
    { id: "second", name: "Portal modded", steamAppId: 400, steamInstallPath: null },
    { id: "manual", name: "Other game", steamAppId: null, steamInstallPath: null },
  ];
  const summaries = new Map([["first", { totalMilliseconds: 100 }], ["second", { totalMilliseconds: 200 }]]);
  const launches = new Map([["second", { status: "running" }]]);
  assert.equal(libraryGroups(games, "").length, 2);
  const modded = libraryItems(libraryGroups(games, "modded"), "modded", summaries, launches);
  assert.equal(modded[0].game.id, "second");
  assert.equal(modded[0].totalMilliseconds, 300);
  const original = libraryItems(libraryGroups(games, "original"), "original", summaries, launches);
  assert.equal(original[0].game.id, "first", "running versions do not override a different query match");
});

test("source indexing preserves verified precedence and release order", async () => {
  const { sourceStatusFor, sourceReleasesFor } = await loadModule("../src/lib/features/store/source-status.ts");
  const manifest = { verified: [{ steamAppId: 400, name: "verified" }, { steamAppId: 400, name: "second" }], unverified: [{ steamAppId: 400, name: "unverified" }] };
  assert.equal(sourceStatusFor(manifest, 400).entry.name, "verified");
  assert.deepEqual(sourceReleasesFor(manifest, 400).map(({ entry }) => entry.name), ["verified", "second", "unverified"]);
  assert.equal(sourceStatusFor(manifest, 401).availability, "unavailable");
  assert.equal(sourceStatusFor(null, 400).availability, "unknown");
});


test("download presentation labels and progress tones follow status", async () => {
  const { statusBadge, phaseLabel, progressTone } = await loadModule("../src/lib/features/downloads/downloads-model.ts", { "../../i18n": localeUrl });
  assert.equal(statusBadge("installed", "en"), "Completed");
  assert.equal(statusBadge("installed", "it"), "Completato");
  assert.equal(phaseLabel({ status: "staged" }, "en"), "Ready to install");
  assert.equal(progressTone("staged"), "accent");
  assert.equal(progressTone("paused"), "warning");
  assert.equal(statusBadge("future", "en"), "future");
});

test("native launch events invalidate stale reads and reduce polling during games", async () => {
  const settings = writable({ data: { hideOnGameStart: false } });
  const info = writable({ data: { trayAvailable: true } });
  const request = deferred();
  let stateListener, failureListener;
  globalThis.legioLaunchEvents = {
    settings, info, active: writable(true),
    list: () => request.promise,
    states: (callback) => { stateListener = callback; return Promise.resolve(() => {}); },
    failure: (callback) => { failureListener = callback; return Promise.resolve(() => {}); },
  };
  const service = dataModule(`
    export const listGameLaunchStates = () => globalThis.legioLaunchEvents.list();
    export const onGameLaunchStates = callback => globalThis.legioLaunchEvents.states(callback);
    export const onShortcutLaunchFailure = callback => globalThis.legioLaunchEvents.failure(callback);
    export const cancelGameLaunch = async () => {};
    export const inspectSteamGameLaunch = async () => ({});
    export const launchSteamGame = async () => {};
    export const stopGame = async () => {};
  `);
  const intervals = [];
  const originalSetInterval = globalThis.setInterval;
  const originalClearInterval = globalThis.clearInterval;
  globalThis.setInterval = (_callback, milliseconds) => { const timer = { milliseconds }; intervals.push(timer); return timer; };
  globalThis.clearInterval = (timer) => { timer.cleared = true; };
  try {
    const { load } = createModuleLoader({ "src/lib/utils/errors": errors });
    const launch = await loadModule("../src/lib/stores/launch.ts", {
      "../services/steam-accounts": service,
      "../utils/errors": errors,
      "./resource": await load("src/lib/stores/resource.ts"),
      "./toast": dataModule("export const showToast = () => {};"),
      "./window-activity": dataModule("export const windowActive = globalThis.legioLaunchEvents.active;"),
      "./app-info": dataModule("export const appInfo = globalThis.legioLaunchEvents.info;"),
      "./settings": dataModule("export const settings = globalThis.legioLaunchEvents.settings;"),
      "../services/launch": dataModule("export const launchConfiguredGameWithRunner = async () => {}; export const launchNativeGame = async () => {};"),
      "../services/game-settings": dataModule("export const getCompatibilityLogsDirectory = async () => '';"),
      "../services/window": dataModule("export const hideWindow = async () => {}; export const showWindow = async () => {};"),
    });
    await launch.startLaunchEvents();
    const pending = launch.launchStates.load();
    stateListener([{ gameId: "game", status: "launching" }]);
    assert.equal(get(launch.hasPendingLaunch), true, "launching still needs state updates");
    assert.equal(get(launch.hasRunningGame), false, "launch preparation must not pause media");
    stateListener([{ gameId: "game", status: "running" }]);
    assert.equal(get(launch.hasRunningGame), true);
    stateListener([{ gameId: "game", status: "launching" }, { gameId: "other", status: "running" }]);
    assert.equal(get(launch.hasRunningGame), true, "any running game suspends media");
    assert.equal(intervals.at(-1).milliseconds, 30000);
    request.resolve([]);
    await pending;
    assert.equal(get(launch.hasPendingLaunch), true, "an old snapshot cannot replace a newer native event");
    globalThis.legioLaunchEvents.active.set(false);
    assert.equal(intervals.at(-1).cleared, true, "native events continue without background UI polling");
    failureListener("shortcut failure");
    assert.equal(get(launch.launchError), "shortcut failure");
    stateListener([{ gameId: "game", status: "idle", error: "launch failed" }]);
    assert.equal(get(launch.hasRunningGame), false, "failed launches leave media active");
    assert.equal(get(launch.hasPendingLaunch), false);
    assert.equal(intervals.at(-1).cleared, true);
  } finally {
    globalThis.setInterval = originalSetInterval;
    globalThis.clearInterval = originalClearInterval;
    delete globalThis.legioLaunchEvents;
  }
});

test("recent hero prefetch pauses during games and retries failures without exceeding two workers", async () => {
  const games = writable({ data: Array.from({ length: 10 }, (_, index) => ({ steamAppId: index + 1 })) });
  const playtime = writable({ data: [] });
  const active = writable(false);
  const pending = writable(false);
  const requests = [];
  globalThis.legioHeroPrefetch = { games, playtime, active, pending, request: (appId) => {
    const response = deferred();
    requests.push({ appId, ...response });
    return response.promise;
  } };
  const { load } = createModuleLoader({
    "src/lib/stores/games": dataModule("export const games = globalThis.legioHeroPrefetch.games;"),
    "src/lib/stores/playtime": dataModule("export const playtime = globalThis.legioHeroPrefetch.playtime;"),
    "src/lib/stores/launch": dataModule("export const hasPendingLaunch = globalThis.legioHeroPrefetch.pending;"),
    "src/lib/stores/window-activity": dataModule("export const windowActive = globalThis.legioHeroPrefetch.active;"),
    "src/lib/services/steam-details": dataModule("export const prefetchSteamHero = appId => globalThis.legioHeroPrefetch.request(appId);"),
    "src/lib/features/home/home-model": dataModule("export const recentGames = games => games.map(game => ({ game }));"),
  });
  const settle = () => new Promise((resolve) => setImmediate(resolve));
  const originalWarn = console.warn;
  const originalNow = Date.now;
  let now = 100000;
  console.warn = () => {};
  Date.now = () => now;
  try {
    const module = await import(await load("src/lib/stores/library-artwork.ts"));
    module.startLibraryHeroCaching();
    assert.equal(requests.length, 0);
    active.set(true);
    assert.deepEqual(requests.map(({ appId }) => appId), [1, 2]);
    pending.set(true);
    requests[0].reject(new Error("temporary failure"));
    requests[1].resolve();
    await settle();
    assert.equal(requests.length, 2, "a game prevents more work when in-flight requests finish");
    active.set(false);
    pending.set(false);
    assert.equal(requests.length, 2);
    now += 30001;
    active.set(true);
    assert.deepEqual(requests.slice(2).map(({ appId }) => appId), [1, 3]);
    active.set(false);
    for (const request of requests.slice(2)) request.resolve();
    await settle();
    assert.equal(requests.length, 4);
  } finally {
    active.set(false);
    console.warn = originalWarn;
    Date.now = originalNow;
    delete globalThis.legioHeroPrefetch;
  }
});

test("hydration waits for startup recovery before loading games and displays failures without a shortcut", async () => {
  const ready = deferred();
  const calls = [];
  const resource = (name) => ({ ...writable({ data: [] }), load: async () => { calls.push(name); } });
  const appInfo = { ...writable({ data: { startupLaunchGameId: null, startupLaunchError: "Startup recovery failed: marker conflict" } }), load: () => { calls.push("appInfo"); return ready.promise; } };
  const settings = writable({ status: "ready", data: { launchInLibrary: false, steamLibraryPollMinutes: 30 } });
  const launchError = writable(null);
  globalThis.legioBootstrap = { appInfo, settings, launchError,
    games: resource("games"), downloads: resource("downloads"), bandwidthLimit: resource("bandwidth"), installedFolder: resource("folder"),
    source: resource("source"), network: resource("network"), launchStates: resource("launch"), playtime: resource("playtime"),
    hasPendingLaunch: writable(false), activeSection: writable("home"),
  };
  const fromGlobal = (...names) => dataModule(names.map((name) => `export const ${name} = globalThis.legioBootstrap.${name};`).join("\n"));
  const { load } = createModuleLoader({
    "src/lib/stores/app-info": fromGlobal("appInfo"),
    "src/lib/stores/settings": fromGlobal("settings"),
    "src/lib/stores/games": fromGlobal("games"),
    "src/lib/stores/playtime": fromGlobal("playtime"),
    "src/lib/stores/library-artwork": dataModule("export const startLibraryHeroCaching = () => {};"),
    "src/lib/stores/downloads": dataModule(["downloads", "bandwidthLimit", "installedFolder"].map((name) => `export const ${name} = globalThis.legioBootstrap.${name};`).join("\n") + "export const startDownloadProgressPolling = () => {};"),
    "src/lib/stores/source-links": dataModule("export const startSourceLinks = async () => {};"),
    "src/lib/stores/source": dataModule("export const source = globalThis.legioBootstrap.source; export const refreshSource = async () => {};"),
    "src/lib/stores/network": dataModule("export const network = globalThis.legioBootstrap.network; export const checkConnectivity = async () => {};"),
    "src/lib/stores/launch": dataModule(["hasPendingLaunch", "launchError", "launchStates"].map((name) => `export const ${name} = globalThis.legioBootstrap.${name};`).join("\n") + "export const startLaunchEvents = async () => {};"),
    "src/lib/stores/steam-library": dataModule("export const importSteamLibrary = async () => {};"),
    "src/lib/stores/navigation": dataModule("export const activeSection = globalThis.legioBootstrap.activeSection; export const openGame = () => {};"),
  });
  const originalInterval = globalThis.setInterval;
  globalThis.setInterval = () => 0;
  try {
    const { hydrateApp } = await import(await load("src/lib/stores/bootstrap.ts"));
    const hydration = hydrateApp();
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(calls, ["appInfo"], "game and folder operations wait for native recovery");
    ready.resolve();
    await hydration;
    assert.ok(calls.includes("games"));
    assert.ok(calls.includes("folder"));
    assert.equal(get(launchError), "Startup recovery failed: marker conflict");
  } finally {
    globalThis.setInterval = originalInterval;
    delete globalThis.legioBootstrap;
  }
});

test("download UI polling stops while inactive and catches completed work on restore", async () => {
  const active = writable(false);
  let jobs = [{ id: "download", status: "downloading" }];
  let requests = 0;
  globalThis.legioDownloadVisibility = { active, invoke: async (command) => {
    assert.equal(command, "list_downloads");
    requests++;
    return jobs;
  } };
  const { load } = createModuleLoader({
    "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.legioDownloadVisibility.invoke(...args);"),
    "src/lib/i18n": localeUrl,
    "src/lib/stores/window-activity": dataModule("export const windowActive = globalThis.legioDownloadVisibility.active;"),
  });
  const timers = new Set();
  const originalSet = globalThis.setInterval;
  const originalClear = globalThis.clearInterval;
  globalThis.setInterval = () => { const timer = {}; timers.add(timer); return timer; };
  globalThis.clearInterval = (timer) => timers.delete(timer);
  try {
    const module = await import(await load("src/lib/stores/downloads.ts"));
    module.downloads.set(jobs);
    module.startDownloadProgressPolling();
    assert.equal(timers.size, 0);
    active.set(true);
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(requests, 1);
    assert.equal(timers.size, 1);
    active.set(false);
    assert.equal(timers.size, 0);
    jobs = [{ id: "download", status: "installed" }];
    assert.equal(requests, 1, "native work can continue without hidden UI requests");
    active.set(true);
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(requests, 2);
    assert.equal(get(module.downloads).data[0].status, "installed");
    assert.equal(timers.size, 0);
  } finally {
    active.set(false);
    globalThis.setInterval = originalSet;
    globalThis.clearInterval = originalClear;
    delete globalThis.legioDownloadVisibility;
  }
});

test("manual artwork refresh renews visible URLs and resets only its Steam App ID", async () => {
  const calls = [];
  let revision = 0;
  globalThis.legioRefreshArtwork = async (command, args) => {
    calls.push([command, args]);
    if (command === "reset_steam_artwork_cache") { revision++; return; }
    return artworkPacket({ bytes: [revision + 1], contentType: "image/png", stale: false, cacheWarning: null, refreshAfter: Date.now() + 100000 });
  };
  const { load } = createModuleLoader({ "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.legioRefreshArtwork(...args);"), "src/lib/i18n": localeUrl });
  const images = await import(await load("src/lib/services/steam-details.ts"));
  const request = (steamAppId) => ({ steamAppId, asset: "logo", fallbackAsset: null, index: null, version: null, full: false });
  const old = await images.loadSteamImage(request(400));
  const untouched = await images.loadSteamImage(request(401));
  const retained = images.retainSteamImage(request(400));
  const first = images.refreshSteamImages(400);
  assert.equal(images.refreshSteamImages(400), first);
  await first;
  assert.notEqual(images.peekSteamImage(request(400)).url, old.url);
  assert.equal(images.peekSteamImage(request(401)).url, untouched.url);
  assert.equal(calls.filter(([command]) => command === "reset_steam_artwork_cache").length, 1);
  assert.ok(calls.filter(([command, args]) => command === "get_steam_asset" && args.refresh).every(([, args]) => args.steamAppId === 400));
  retained.release();
});
