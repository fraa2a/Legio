import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";
const { load } = createModuleLoader({
  "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.detailsInvoke(...args);"),
  "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language = writable("en"); export const t = s => s;`),
});
test("cached metadata is visible while page-open refresh is pending", async () => {
  const store = await import(await load("src/lib/stores/steam-details.ts"));
  let finish; const policies = [];
  globalThis.detailsInvoke = async (cmd, args) => {
    if (cmd === "cancel_artwork_request") return;
    policies.push(args.policy);
    if (args.policy === "refresh_if_expired") return new Promise(resolve => { finish = resolve; });
    return { details: { name: "OLD" }, cachedAt: 1, stale: true };
  };
  const work = { requestId: "page", origin: "visible", release() {} };
  const opening = store.refreshSteamDetailsOnOpen(55, work);
  for (let i = 0; i < 20; i++) await Promise.resolve();
  assert.equal(get(store.steamDetails)[55].details.name, "OLD");
  assert.equal(get(store.steamDetails)[55].status, "ready");
  finish({ details: { name: "NEW" }, cachedAt: 2, stale: false }); await opening;
  assert.equal(get(store.steamDetails)[55].details.name, "NEW");
  assert.deepEqual(policies, ["cache_or_fetch", "refresh_if_expired"]);
});
test("localized_refresh_does_not_invalidate_artwork", async () => {
  const images = await import(await load("src/lib/services/steam-details.ts"));
  const store = await import(await load("src/lib/stores/steam-details.ts"));
  const { artworkPacket } = await import("./test_module_loader.mjs");
  const request = { steamAppId: 56, asset: "hero", fallbackAsset: null, index: null, full: false, version: 1 };
  const calls = [];
  globalThis.detailsInvoke = async cmd => {
    calls.push(cmd);
    if (cmd === "get_steam_asset") return artworkPacket({ bytes: [1, 2], contentType: "image/png", stale: false, cacheWarning: null, refreshAfter: Date.now()+60_000 });
    if (cmd === "get_steam_details") return { details: { name: "NEW" }, cachedAt: 2, stale: false };
  };
  const first = await images.loadSteamImage(request);
  await store.loadSteamDetails(56, "force_refresh");
  assert.equal(images.peekSteamImage(request).url, first.url);
  assert.equal(calls.filter(cmd => cmd === "get_steam_asset").length, 1);
});
test("cancelled metadata work does not replace the current state with an error", async () => {
  const store = await import(await load("src/lib/stores/steam-details.ts"));
  globalThis.detailsInvoke = async cmd => { if (cmd === "get_steam_details") throw { kind: "cancelled", message: "Artwork request cancelled" }; };
  await assert.rejects(store.loadSteamDetails(57, "cache_or_fetch"));
  assert.equal(get(store.steamDetails)[57].error, null);
});

test("reopening a game does not reuse cancelled metadata work", async () => {
  const store = await import(await load("src/lib/stores/steam-details.ts"));
  const { createArtworkWork } = await import(await load("src/lib/services/artwork-work.ts"));
  const waiting = [];
  globalThis.detailsInvoke = (command) => command === "get_steam_details" ? new Promise(resolve => waiting.push(resolve)) : Promise.resolve();
  const oldWork = createArtworkWork(999, "visible");
  const old = store.loadSteamDetails(999, "refresh_if_expired", oldWork);
  oldWork.release();
  const newWork = createArtworkWork(999, "visible");
  const fresh = store.loadSteamDetails(999, "refresh_if_expired", newWork);
  try {
    assert.equal(waiting.length, 2, "a reopened game needs its own live metadata request");
    waiting[1]({ details: { name: "NEW" }, cachedAt: 2, stale: false }); await fresh;
    waiting[0]({ details: { name: "OLD" }, cachedAt: 1, stale: true }); await old;
    assert.equal(get(store.steamDetails)[999].details.name, "NEW");
  } finally { newWork.release(); }
});

test("visible metadata demand cancels only when its last consumer leaves", async () => {
  const store = await import(await load("src/lib/stores/steam-details.ts"));
  let finish; let requestId; const cancelled = [];
  globalThis.detailsInvoke = (command, args) => {
    if (command === "cancel_artwork_request") { cancelled.push(args.requestId); return Promise.resolve(); }
    if (command === "get_steam_details") { requestId = args.requestId; return new Promise(resolve => { finish = resolve; }); }
  };
  const releaseA = store.ensureSteamDetails(818);
  const releaseB = store.ensureSteamDetails(818);
  assert.equal(typeof releaseA, "function", "visible metadata must provide lifetime cleanup");
  releaseA();
  assert.ok(!cancelled.includes(requestId), "one remaining consumer still needs this request");
  releaseB();
  assert.ok(cancelled.includes(requestId), "the final offscreen consumer must cancel native demand");
  finish({ details: { name: "OBSOLETE" }, cachedAt: 1, stale: false });
  for (let i=0;i<20;i++) await Promise.resolve();
  assert.equal(get(store.steamDetails)[818].details, null, "abandoned result must not publish");
  assert.equal(get(store.steamDetails)[818].status, "idle", "returning row can retry");
});
