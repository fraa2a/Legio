import assert from "node:assert/strict";
import { test } from "node:test";
import { artworkPacket, createModuleLoader, dataModule } from "./test_module_loader.mjs";

const { load } = createModuleLoader({
  "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.cacheInvoke(...args);"),
  "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language = writable("en");`),
});
const images = await import(await load("src/lib/services/steam-details.ts"));
const request = (steamAppId, asset, fallbackAsset = null) => ({ steamAppId, asset, fallbackAsset, index: null, version: null, full: false });
const packet = () => artworkPacket({ bytes: [1, 2], contentType: "image/png", stale: false, cacheWarning: null, refreshAfter: Date.now() + 60_000 });

test("identical assets share an in-flight download and object URL across fallback policies", async () => {
  const completions = [];
  globalThis.cacheInvoke = () => new Promise(resolve => completions.push(resolve));
  const a = images.loadSteamImage(request(903, "hero"));
  const b = images.loadSteamImage(request(903, "hero", "header"));
  assert.equal(completions.length, 1, "fallback choice must not duplicate the primary request");
  completions[0](packet());
  const [first, second] = await Promise.all([a, b]);
  assert.equal(first.url, second.url);
});

test("a fallback reuses the header cache without substituting it for a caller requiring a hero", async () => {
  const calls = [];
  globalThis.cacheInvoke = (_command, args) => {
    calls.push(args.asset);
    return args.asset === "hero" ? Promise.reject(new Error("missing hero")) : Promise.resolve(packet());
  };
  const header = await images.loadSteamImage(request(904, "header"));
  const fallback = await images.loadSteamImage(request(904, "hero", "header"));
  assert.equal(fallback.url, header.url);
  assert.deepEqual(calls, ["header", "hero"]);
  assert.equal(images.peekSteamImage(request(904, "hero")), undefined);
  assert.equal(images.peekSteamImage(request(904, "hero", "header")).url, header.url);
  await assert.rejects(images.loadSteamImage(request(904, "hero")), /missing hero/);
});

test("manual refresh retries the original asset after a fallback was displayed", async () => {
  let missing = true;
  globalThis.cacheInvoke = (command, args) => {
    if (command === "reset_steam_artwork_cache") { missing = false; return Promise.resolve(); }
    if (args.asset === "hero" && missing) return Promise.reject(new Error("missing hero"));
    return Promise.resolve(packet());
  };
  const heroRequest = request(905, "hero", "header");
  const previous = await images.loadSteamImage(heroRequest);
  const retained = images.retainSteamImage(heroRequest);
  await images.refreshSteamImages(905);
  assert.ok(images.peekSteamImage(request(905, "hero")), "manual refresh must retry the original hero");
  assert.notEqual(images.peekSteamImage(heroRequest).url, previous.url);
  retained.release();
});

test("manual refresh keeps a missing hero fallback in the shared header cache", async () => {
  globalThis.cacheInvoke = (command, args) => {
    if (command === "reset_steam_artwork_cache") return Promise.resolve();
    return args.asset === "hero" ? Promise.reject(new Error("missing hero")) : Promise.resolve(packet());
  };
  const heroRequest = request(906, "hero", "header");
  await images.loadSteamImage(heroRequest);
  await images.refreshSteamImages(906);
  assert.equal(images.peekSteamImage(request(906, "hero")), undefined);
  assert.equal(images.peekSteamImage(heroRequest).url, images.peekSteamImage(request(906, "header")).url);
});

test("direct artwork consumers share an in-flight refresh", async () => {
  const completions = [];
  globalThis.cacheInvoke = () => new Promise(resolve => completions.push(resolve));
  const a = images.getSteamAsset(907, "header", undefined, false, true);
  const b = images.getSteamAsset(907, "header", undefined, false, true);
  assert.equal(completions.length, 1);
  completions[0](packet());
  const [first, second] = await Promise.all([a, b]);
  assert.equal(first, second);
});

test("manual refresh does not put current-monitor bytes in an old fallback profile", async () => {
  const { artworkDisplayWidth } = await import(await load("src/lib/stores/artwork-display.ts"));
  const calls = [];
  globalThis.cacheInvoke = (command, args) => {
    if (command === "reset_steam_artwork_cache") return Promise.resolve();
    calls.push(args.asset);
    return args.asset === "hero" ? Promise.reject(new Error("missing hero")) : Promise.resolve(packet());
  };
  await images.loadSteamImage({ ...request(908, "hero", "header"), displayWidth: 1920 });
  calls.length = 0;
  artworkDisplayWidth.set(2560);
  try {
    await images.refreshSteamImages(908);
    assert.deepEqual(calls, ["header"]);
  } finally { artworkDisplayWidth.set(1920); }
});

test("an expired fallback retries the original asset and publishes its recovered image", async () => {
  let missing = true;
  const calls = [];
  globalThis.cacheInvoke = (_command, args) => {
    calls.push(args.asset);
    if (args.asset === "hero" && missing) return Promise.reject(new Error("missing hero"));
    return Promise.resolve(packet());
  };
  const heroRequest = request(909, "hero", "header");
  await images.loadSteamImage(heroRequest);
  missing = false;
  const now = Date.now;
  Date.now = () => now() + 61_000;
  try {
    await images.loadSteamImage(heroRequest);
    for (let i = 0; i < 4; i++) await new Promise(resolve => setImmediate(resolve));
    assert.ok(images.peekSteamImage(request(909, "hero")));
    assert.deepEqual(calls, ["hero", "header", "hero"]);
  } finally { Date.now = now; }
});

test("releasing a recovered primary does not evict a fallback still used by another consumer", async () => {
  let missing = true;
  const revoked = [];
  const revoke = URL.revokeObjectURL;
  URL.revokeObjectURL = url => { revoked.push(url); revoke(url); };
  globalThis.cacheInvoke = (_command, args) => args.asset === "hero" && missing
    ? Promise.reject(new Error("missing hero")) : Promise.resolve(packet());
  const aRequest = request(910, "hero", "header");
  const bRequest = request(910, "hero", "header");
  try {
    await images.loadSteamImage(aRequest);
    const a = images.retainSteamImage(aRequest);
    missing = false;
    await images.loadSteamImage(request(910, "hero"));
    const b = images.retainSteamImage(bRequest);
    b.release();
    for (let i = 0; i < 130; i++) await images.loadSteamImage(request(10000 + i, "header"));
    assert.equal(revoked.includes(a.url), false, "the live fallback must remain retained");
    a.release();
  } finally { URL.revokeObjectURL = revoke; }
});

test("refreshing a shared header cannot postpone another consumer's primary recovery", async () => {
  let missing = true;
  const calls = [];
  const now = Date.now;
  globalThis.cacheInvoke = (_command, args) => {
    calls.push(args.asset);
    if (args.asset === "hero" && missing) return Promise.reject(new Error("missing hero"));
    return Promise.resolve(packet());
  };
  const heroRequest = request(911, "hero", "header");
  await images.loadSteamImage(heroRequest);
  missing = false;
  Date.now = () => now() + 61_000;
  try {
    await Promise.all([images.loadSteamImage(request(911, "header")), images.loadSteamImage(heroRequest)]);
    for (let i = 0; i < 4; i++) await new Promise(resolve => setImmediate(resolve));
    assert.ok(images.peekSteamImage(request(911, "hero")));
    assert.ok(calls.slice(2).includes("hero"));
  } finally { Date.now = now; }
});

test("retained fallback reports its retry deadline after a failed refresh", async () => {
  let offline = false;
  const now = Date.now;
  globalThis.cacheInvoke = (_command, args) => args.asset === "hero" || offline
    ? Promise.reject(new Error("offline")) : Promise.resolve(packet());
  const heroRequest = request(912, "hero", "header");
  await images.loadSteamImage(heroRequest);
  const expired = now() + 61_000;
  Date.now = () => expired;
  offline = true;
  try {
    await images.loadSteamImage(heroRequest);
    for (let i = 0; i < 4; i++) await new Promise(resolve => setImmediate(resolve));
    const retained = images.retainSteamImage(heroRequest);
    try { assert.equal(retained.refreshAt, expired + 300_000); }
    finally { retained.release(); }
  } finally { Date.now = now; }
});
