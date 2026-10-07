import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};
const empty = { manifest: null, cachedAt: null, stale: false, warning: null, sources: [] };
const installed = { ...empty, manifest: { schemaVersion: 1, generatedAt: "2026-09-22T00:00:00Z", verified: [], unverified: [] }, sources: [{ id: "one", url: "https://example.invalid/games.json", cachedAt: 100, stale: false, warning: null, gameCount: 0 }] };

function fixture(name, invoke, listen = async () => () => {}) {
  globalThis[name] = { invoke, listen, sections: [], invalidations: 0 };
  return createModuleLoader({
    "src/lib/i18n": dataModule("export const t = value => value;"),
    "@tauri-apps/api/core": dataModule(`export const invoke = (...args) => globalThis.${name}.invoke(...args);`),
    "@tauri-apps/api/event": dataModule(`export const listen = (...args) => globalThis.${name}.listen(...args);`),
    "src/lib/services/catalog": dataModule(`export const invalidateCatalogAvailability = () => { globalThis.${name}.invalidations++; };`),
    "src/lib/stores/navigation": dataModule(`export const openSettings = section => globalThis.${name}.sections.push(section);`),
  });
}

test("source mutations wait for an older refresh and removals invalidate its availability", async () => {
  const refresh = deferred();
  const calls = [];
  const { load } = fixture("sourceSerial", async (command) => {
    calls.push(command);
    if (command === "refresh_legio_source") return refresh.promise;
    if (command === "remove_download_source") return empty;
    throw new Error(command);
  });
  const store = await import(await load("src/lib/stores/source.ts"));
  const refreshing = store.refreshSource();
  await new Promise(setImmediate);
  const removing = store.removeSource("one");
  await new Promise(setImmediate);
  assert.deepEqual(calls, ["refresh_legio_source"]);
  refresh.resolve(installed);
  await Promise.all([refreshing, removing]);
  assert.deepEqual(calls, ["refresh_legio_source", "remove_download_source"]);
  assert.equal(get(store.source).data.manifest, null);
  assert.equal(get(store.sourceBusy), false);
  assert.equal(globalThis.sourceSerial.invalidations, 2);
});

test("failed additions preserve installed sources and allow retry", async () => {
  let fail = true;
  const { load } = fixture("sourceRetry", async () => {
    if (fail) throw new Error("Invalid manifest");
    return installed;
  });
  const store = await import(await load("src/lib/stores/source.ts"));
  store.source.set(installed);
  assert.equal(await store.addSource("https://example.invalid/games.json"), false);
  assert.equal(get(store.source).data.sources.length, 1);
  assert.equal(get(store.sourceActionError), "Invalid manifest");
  fail = false;
  assert.equal(await store.addSource("https://example.invalid/games.json"), true);
  assert.equal(get(store.sourceActionError), null);
});

test("startup links and links arriving during installation are processed once", async () => {
  let listener;
  let reads = 0;
  const first = deferred();
  const adds = [];
  const queue = [{ url: "https://example.invalid/one.json", error: null }];
  const { load } = fixture("sourceLinks", async (command, args) => {
    if (command === "take_source_links") {
      reads++;
      return { requests: queue.splice(0), registrationError: null };
    }
    if (command === "add_download_source") {
      adds.push(args.url);
      return adds.length === 1 ? first.promise : installed;
    }
    throw new Error(command);
  }, async (_event, callback) => { listener = callback; return () => {}; });
  const links = await import(await load("src/lib/stores/source-links.ts"));
  const startup = links.startSourceLinks();
  await new Promise(setImmediate);
  queue.push({ url: "https://example.invalid/two.json", error: null });
  listener();
  first.resolve(installed);
  await startup;
  await links.startSourceLinks();
  assert.deepEqual(adds, ["https://example.invalid/one.json", "https://example.invalid/two.json"]);
  assert.equal(reads, 2);
  assert.deepEqual(globalThis.sourceLinks.sections, ["sources", "sources"]);
});

test("invalid external links display an error without adding a source", async () => {
  const calls = [];
  const { load } = fixture("sourceInvalidLink", async (command) => {
    calls.push(command);
    return { requests: [{ url: null, error: "Invalid source link" }], registrationError: "Protocol registration failed" };
  });
  const links = await import(await load("src/lib/stores/source-links.ts"));
  const store = await import(await load("src/lib/stores/source.ts"));
  await links.startSourceLinks();
  assert.deepEqual(calls, ["take_source_links"]);
  assert.equal(get(store.sourceActionError), "Invalid source link");
  assert.equal(get(links.sourceLinkRegistrationError), "Protocol registration failed");
});
