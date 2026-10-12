import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const defaults = { sort: "relevance", availability: "all" };
const filtered = { sort: "name_desc", availability: "verified" };
const result = (id, nextOffset = null, total = 1) => ({
  games: [{ steamAppId: id, name: `Game ${id}`, availability: "verified" }],
  total, nextOffset, cachedAt: 1, stale: false, sourceCachedAt: 1, sourceStale: false,
});

function fixture(name, invoke) {
  globalThis[name] = invoke;
  return createModuleLoader({
    "@tauri-apps/api/core": dataModule(`export const invoke = (...args) => globalThis.${name}(...args);`),
    "src/lib/utils/errors": dataModule("export const toMessage = error => String(error);"),
    "src/lib/stores/source": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const source = writable({ data: { manifest: null, stale: false } });`),
  });
}

test("catalog sends sorting and availability through local and remote pagination", async () => {
  const calls = [];
  const { load } = fixture("catalogOptions", async (command, args) => { calls.push([command, args]); return result(1); });
  const service = await import(await load("src/lib/services/catalog.ts"));
  await service.searchCatalog("portal", 20, filtered);
  await service.refreshCatalog("portal", 50, 40, filtered);
  assert.deepEqual(calls, [
    ["search_catalog", { query: "portal", limit: 20, options: filtered }],
    ["refresh_catalog", { query: "portal", skip: 50, limit: 40, options: filtered }],
  ]);
});

test("catalog caches and pending requests are isolated by both filters", async () => {
  const requests = [];
  const { load } = fixture("catalogCacheOptions", (_command, args) => new Promise(resolve => requests.push({ args, resolve })));
  const service = await import(await load("src/lib/services/catalog.ts"));
  const first = service.refreshCatalogCached("Portal", defaults);
  const again = service.refreshCatalogCached(" portal ", defaults);
  const differentSort = service.refreshCatalogCached("portal", { ...defaults, sort: "name_asc" });
  const differentAvailability = service.refreshCatalogCached("portal", filtered);
  assert.equal(first, again);
  assert.equal(requests.length, 3);
  requests.forEach((request, index) => request.resolve(result(index + 1)));
  await Promise.all([first, differentSort, differentAvailability]);
  assert.equal(service.cachedCatalogSearch("PORTAL", defaults).games[0].steamAppId, 1);
  assert.equal(service.cachedCatalogSearch("portal", { ...defaults, sort: "name_asc" }).games[0].steamAppId, 2);
  assert.equal(service.cachedCatalogSearch("portal", filtered).games[0].steamAppId, 3);
});

test("changing a source prevents an older refresh from repopulating the cache", async () => {
  let finish;
  const { load } = fixture("catalogSourceInvalidation", () => new Promise(resolve => { finish = resolve; }));
  const service = await import(await load("src/lib/services/catalog.ts"));
  const request = service.refreshCatalogCached("portal", filtered);
  service.invalidateCatalogAvailability();
  finish(result(1));
  await request;
  assert.equal(service.cachedCatalogSearch("portal", filtered), null);
});

test("switching filters ignores an older search and keeps filters when loading more", async () => {
  const remote = [];
  const calls = [];
  const { load } = fixture("catalogStoreOptions", async (command, args) => {
    calls.push([command, args]);
    if (command === "search_catalog") return result(args.limit ? 3 : 1);
    return new Promise(resolve => remote.push({ args, resolve }));
  });
  const store = await import(await load("src/lib/stores/catalog.ts"));
  const first = store.runCatalogSearch("portal", defaults);
  await new Promise(setImmediate);
  const second = store.runCatalogSearch("portal", filtered);
  await new Promise(setImmediate);
  assert.equal(remote.length, 2);
  remote[1].resolve(result(2, 50, 2));
  await second;
  remote[0].resolve(result(99));
  await first;
  assert.equal(get(store.catalog).results[0].steamAppId, 2);
  await store.loadMoreCatalog("portal", 40);
  assert.deepEqual(calls.at(-1), ["search_catalog", { query: "portal", limit: 40, options: filtered }]);
  const next = store.loadMoreCatalog("portal", 60);
  await new Promise(setImmediate);
  assert.equal(remote.at(-1).args.skip, 50);
  assert.deepEqual(remote.at(-1).args.options, filtered);
  remote.at(-1).resolve(result(4));
  await next;
  assert.equal(get(store.catalog).results[0].steamAppId, 4);
});

test("refreshing without explicit options preserves the current store selection", async () => {
  const calls = [];
  const { load } = fixture("catalogHeaderRefresh", async (command, args) => { calls.push([command, args]); return result(1); });
  const store = await import(await load("src/lib/stores/catalog.ts"));
  await store.runCatalogSearch("portal", filtered);
  await store.runCatalogSearch("portal");
  assert.deepEqual(get(store.catalog).options, filtered);
  assert.ok(calls.every(([, args]) => args.options.sort === "name_desc" && args.options.availability === "verified"));
});

test("source changes invalidate a local load-more response before the next search starts", async () => {
  let finish;
  const { load } = fixture("catalogLocalInvalidation", async (command, args) => {
    if (command === "search_catalog" && args.limit) return new Promise(resolve => { finish = resolve; });
    return result(1, null, 2);
  });
  const store = await import(await load("src/lib/stores/catalog.ts"));
  const service = await import(await load("src/lib/services/catalog.ts"));
  const { source } = await import(await load("src/lib/stores/source"));
  await store.runCatalogSearch("portal", filtered);
  const loading = store.loadMoreCatalog("portal", 40);
  await new Promise(setImmediate);
  service.invalidateCatalogAvailability();
  source.set({ data: { manifest: { verified: [], unverified: [] }, stale: false } });
  finish(result(99));
  await loading;
  assert.equal(service.cachedCatalogSearch("portal", filtered), null);
  assert.equal(get(store.catalog).results[0].steamAppId, 1);
});

test("retry after a failed remote page bypasses cached local results and preserves filters", async () => {
  const calls = [];
  const { load } = fixture("catalogPageRetry", async (command, args) => {
    calls.push([command, args]);
    if (command === "refresh_catalog" && args.skip > 0) throw new Error("Offline");
    if (command === "search_catalog" && args.limit) return result(2, 50, 1);
    return result(1, 50, 2);
  });
  const store = await import(await load("src/lib/stores/catalog.ts"));
  await store.runCatalogSearch("portal", filtered);
  await store.loadMoreCatalog("portal", 40);
  await store.loadMoreCatalog("portal", 60);
  assert.match(get(store.catalog).error, /Offline/);
  const before = calls.length;
  await store.runCatalogSearch("portal", filtered, true);
  assert.deepEqual(calls.slice(before), [
    ["search_catalog", { query: "portal", limit: undefined, options: filtered }],
    ["refresh_catalog", { query: "portal", skip: 0, limit: undefined, options: filtered }],
  ]);
  assert.equal(get(store.catalog).error, null);
});
