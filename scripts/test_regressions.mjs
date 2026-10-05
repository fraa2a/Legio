import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
import { get } from "svelte/store";

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
    return Promise.resolve({ bytes: [1, 2], contentType: "image/png" });
  };
  const api = dataModule("export const invoke = (...args) => globalThis.legioInvokeMock(...args);");
  const { acquireGameArtwork, setGameIcon } = await loadModule("../src/lib/services/game-artwork.ts", { "@tauri-apps/api/core": api });
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

test("Steam image cache distinguishes metadata versions", async () => {
  let requests = 0;
  globalThis.legioInvokeMock = () => {
    requests++;
    return Promise.resolve({ bytes: [1], contentType: "image/png", stale: false, cacheWarning: null, refreshAfter: Date.now() + 60_000 });
  };
  const api = dataModule("export const invoke = (...args) => globalThis.legioInvokeMock(...args);");
  const { loadSteamImage } = await loadModule("../src/lib/services/steam-details.ts", { "@tauri-apps/api/core": api });
  const request = { steamAppId: 999, asset: "hero", fallbackAsset: null, index: null, version: 1, full: false };
  const oldImage = await loadSteamImage(request);
  const newImage = await loadSteamImage({ ...request, version: 2 });
  assert.equal(requests, 2);
  assert.notEqual(oldImage.url, newImage.url);
  URL.revokeObjectURL(oldImage.url);
  URL.revokeObjectURL(newImage.url);
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
  const { catalog: state, runCatalogSearch, loadMoreCatalog } = await loadModule("../src/lib/stores/catalog.ts", { "../services/catalog": service, "../utils/errors": errors });
  await runCatalogSearch("portal");
  assert.equal(get(state).nextOffset, 50);
  await loadMoreCatalog("portal", 40);
  assert.equal(get(state).nextOffset, 50);
  await loadMoreCatalog("portal", 60);
  assert.deepEqual(pages, [50]);
  assert.equal(get(state).results.length, 3);
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
