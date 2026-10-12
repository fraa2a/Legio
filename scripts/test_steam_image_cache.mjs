import assert from "node:assert/strict";
import { test } from "node:test";
import { artworkPacket, createModuleLoader, dataModule } from "./test_module_loader.mjs";

const { load } = createModuleLoader({
  "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.cacheInvoke(...args);"),
  "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language = writable("en"); export const t = s => s;`),
});
const images = await import(await load("src/lib/services/steam-details.ts"));
const logging = await import(await load("src/lib/services/application-log.ts"));
logging.setApplicationLoggingEnabled(false);
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
    if (_command === "cancel_artwork_request") return Promise.resolve();
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
    if (command === "cancel_artwork_request") return Promise.resolve();
    if (args.policy === "force_refresh") missing = false;
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
    if (command === "cancel_artwork_request") return Promise.resolve();
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
    if (command === "cancel_artwork_request") return Promise.resolve();
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

test("page opening with an expired fallback retries the original asset and publishes its recovered image", async () => {
  let missing = true;
  const calls = [];
  globalThis.cacheInvoke = (_command, args) => {
    if (_command === "cancel_artwork_request") return Promise.resolve();
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
    await images.refreshSteamImagesOnOpen(909, { requestId: "open-909", origin: "visible", release() {} });
    assert.ok(images.peekSteamImage(request(909, "hero")));
    assert.deepEqual(calls, ["hero", "header", "header", "hero"]);
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
    if (_command === "cancel_artwork_request") return Promise.resolve();
    calls.push(args.asset);
    if (args.asset === "hero" && missing) return Promise.reject(new Error("missing hero"));
    return Promise.resolve(packet());
  };
  const heroRequest = request(911, "hero", "header");
  await images.loadSteamImage(heroRequest);
  missing = false;
  Date.now = () => now() + 61_000;
  try {
    await images.refreshSteamImagesOnOpen(911, { requestId: "open-911", origin: "visible", release() {} });
    assert.ok(images.peekSteamImage(request(911, "hero")));
    assert.ok(calls.slice(2).includes("hero"));
  } finally { Date.now = now; }
});

test("failed page-open refresh preserves the fallback successful-fetch age", async () => {
  let offline = false;
  const now = Date.now;
  globalThis.cacheInvoke = (_command, args) => args.asset === "hero" || offline
    ? Promise.reject(new Error("offline")) : Promise.resolve(packet());
  const heroRequest = request(912, "hero", "header");
  await images.loadSteamImage(heroRequest);
  const previousAge = images.peekSteamImage(heroRequest).refreshAt;
  const expired = now() + 61_000;
  Date.now = () => expired;
  offline = true;
  try {
    await assert.rejects(images.refreshSteamImagesOnOpen(912, { requestId: "open-912", origin: "visible", release() {} }));
    const retained = images.retainSteamImage(heroRequest);
    try { assert.equal(retained.refreshAt, previousAge); }
    finally { retained.release(); }
  } finally { Date.now = now; }
});

const settle = () => new Promise(resolve => setImmediate(resolve));
const stagedPacket = token => artworkPacket({ bytes: [1, 2], contentType: "image/webp", stale: false, cacheWarning: null, refreshAfter: Date.now() + 432_000_000, replacementToken: token });
test("cached_image_survives_pending_refresh", async () => {
  const imageRequest = request(930, "hero"); const calls = []; let complete;
  globalThis.cacheInvoke = (command, args) => {
    calls.push(command);
    if (command === "get_steam_asset" && args.policy === "force_refresh") return new Promise(resolve => { complete = resolve; });
    return Promise.resolve(packet());
  };
  const old = await images.loadSteamImage(imageRequest);
  const refresh = images.refreshSteamImages(930); await settle();
  assert.equal(images.peekSteamImage(imageRequest).url, old.url);
  assert.ok(!calls.includes("reset_steam_artwork_cache"));
  complete(packet()); await refresh;
});
test("replacement_waits_for_display_decode", async () => {
  const imageRequest = request(931, "hero"); const old = await images.loadSteamImage(imageRequest);
  let decode; const acknowledgements = [];
  globalThis.Image = class { decode() { return new Promise(resolve => { decode = resolve; }); } };
  globalThis.cacheInvoke = async (command, args) => {
    if (command === "get_steam_asset") return stagedPacket("candidate-931");
    if (command === "finish_steam_asset_update") { acknowledgements.push(args); return { cacheWarning: null }; }
  };
  try {
    const refresh = images.refreshSteamImages(931); await settle();
    assert.equal(images.peekSteamImage(imageRequest).url, old.url);
    assert.equal(acknowledgements.length, 0);
    decode(); await refresh;
    assert.notEqual(images.peekSteamImage(imageRequest).url, old.url);
    assert.deepEqual(acknowledgements, [{ token: "candidate-931", outcome: "displayed" }]);
  } finally { delete globalThis.Image; }
});
test("releasing_old_generation_does_not_revoke_another_consumer", async () => {
  globalThis.cacheInvoke = async command => command === "finish_steam_asset_update" ? { cacheWarning: null } : packet();
  const imageRequest = request(932, "header"); const old = await images.loadSteamImage(imageRequest);
  const a = images.retainSteamImage(imageRequest); const b = images.retainSteamImage(imageRequest);
  const revoked = []; const original = URL.revokeObjectURL; URL.revokeObjectURL = url => { revoked.push(url); original(url); };
  try {
    await images.refreshSteamImages(932); a.release(); assert.ok(!revoked.includes(old.url));
    b.release(); assert.ok(revoked.includes(old.url));
  } finally { URL.revokeObjectURL = original; }
});
test("details_timestamp_change_preserves_artwork_identity", async () => {
  let calls = 0; globalThis.cacheInvoke = async command => { if (command === "get_steam_asset") { calls++; return packet(); } };
  const first = await images.loadSteamImage({ ...request(933, "header"), version: 1 });
  const second = await images.loadSteamImage({ ...request(933, "header"), version: 2 });
  assert.equal(first.url, second.url); assert.equal(calls, 1);
  const screenshot = { ...request(934, "screenshot"), index: 0, screenshotIdentity: "https://cdn.steamstatic.com/a.jpg" };
  await images.loadSteamImage(screenshot); await images.loadSteamImage({ ...screenshot, screenshotIdentity: "https://cdn.steamstatic.com/b.jpg" }); assert.equal(calls, 3);
});
test("consumerless_request_cancels_native_work", async () => {
  const calls = []; let complete;
  globalThis.cacheInvoke = (command, args) => { calls.push([command, args]); if (command === "get_steam_asset") return new Promise(resolve => { complete = resolve; }); return Promise.resolve({ cacheWarning: null }); };
  const a = images.acquireSteamImage(request(935, "hero"), "visible"); const b = images.acquireSteamImage(request(935, "hero"), "visible");
  a.release(); assert.equal(calls.filter(([cmd]) => cmd === "cancel_artwork_request").length, 0);
  b.release(); assert.equal(calls.filter(([cmd]) => cmd === "cancel_artwork_request").length, 1);
  complete(stagedPacket("abandoned-935")); await Promise.allSettled([a.promise, b.promise]);
  assert.ok(calls.some(([cmd, args]) => cmd === "finish_steam_asset_update" && args.outcome === "abandoned"));
});
test("rejected replacement retains the decoded previous image", async () => {
  globalThis.cacheInvoke = async () => packet(); const imageRequest = request(936, "hero"); const old = await images.loadSteamImage(imageRequest);
  const outcomes = []; globalThis.Image = class { async decode() { throw new Error("decode failed"); } };
  globalThis.cacheInvoke = async (command, args) => { if (command === "get_steam_asset") return stagedPacket("rejected-936"); if (command === "finish_steam_asset_update") { outcomes.push(args.outcome); return { cacheWarning: null }; } };
  try { await assert.rejects(images.refreshSteamImages(936)); assert.equal(images.peekSteamImage(imageRequest).url, old.url); assert.deepEqual(outcomes, ["rejected"]); } finally { delete globalThis.Image; }
});
test("an opened game checks assets that finish their initial cached read later", async () => {
  const policies = []; let update; const controller = new AbortController();
  globalThis.cacheInvoke = async (command, args) => {
    if (command !== "get_steam_asset") return { cacheWarning: null };
    policies.push(args.policy);
    if (args.policy === "refresh_if_expired") return new Promise(resolve => { update = resolve; });
    return artworkPacket({ bytes: [1, 2], contentType: "image/png", stale: true, cacheWarning: null, refreshAfter: 0 });
  };
  await images.refreshSteamImagesOnOpen(941, { requestId: "open-941", origin: "visible", signal: controller.signal, release() {} });
  const consumer = images.acquireSteamImage(request(941, "hero"), "visible");
  const old = await consumer.promise; await settle();
  assert.deepEqual(policies, ["cache_or_fetch", "refresh_if_expired"]);
  assert.equal(images.peekSteamImage(request(941, "hero")).url, old.url);
  update(packet()); await settle(); consumer.release(); controller.abort();
});

test("artwork phase logging is opt-in and cannot delay image publication", async () => {
  const records = [];
  globalThis.cacheInvoke = (command, args) => {
    if (command === "report_application_event") { records.push(args); return new Promise(() => {}); }
    return Promise.resolve(packet());
  };
  logging.setApplicationLoggingEnabled(false);
  await images.loadSteamImage(request(996, "hero"));
  assert.equal(records.length, 0);
  logging.setApplicationLoggingEnabled(true);
  try {
    const image = await images.loadSteamImage(request(997, "hero"));
    assert.ok(image.url); assert.equal(records.length, 1);
    assert.equal(records[0].event, "artwork");
    assert.match(records[0].message, /publication_ms=/);
    assert.ok(!records[0].message.includes("blob:") && !records[0].message.includes("https:") && !records[0].message.includes("997"));
  } finally { logging.setApplicationLoggingEnabled(false); }
});

test("reopening a game replaces cancelled refresh demand", async () => {
  const { createArtworkWork } = await import(await load("src/lib/services/artwork-work.ts"));
  const waiting = [];
  globalThis.cacheInvoke = (command, args) => {
    if (command !== "get_steam_asset") return Promise.resolve();
    return args.policy === "cache_or_fetch" ? Promise.resolve(packet()) : new Promise(resolve => waiting.push(resolve));
  };
  await images.loadSteamImage(request(998, "hero"));
  const oldWork = createArtworkWork(998, "visible"); const old = images.refreshSteamImagesOnOpen(998, oldWork);
  oldWork.release();
  const work = createArtworkWork(998, "visible"); const current = images.refreshSteamImagesOnOpen(998, work);
  try {
    for (let i=0;i<10;i++) await Promise.resolve();
    assert.equal(waiting.length, 2, "a new opening must not inherit abandoned refresh work");
    waiting[1](packet()); await current;
    waiting[0](packet()); await old;
  } finally { work.release(); }
});

test("explicit refresh waits for an opening check then performs a force fetch", async () => {
  const policies = []; let finish;
  globalThis.cacheInvoke = (command, args) => {
    if (command !== "get_steam_asset") return Promise.resolve();
    policies.push(args.policy);
    return args.policy === "refresh_if_expired" ? new Promise(resolve => { finish = resolve; }) : Promise.resolve(packet());
  };
  await images.loadSteamImage(request(995, "hero"));
  const opening = images.refreshSteamImagesOnOpen(995, { requestId: "open-force", origin: "visible", release() {} });
  const explicit = images.refreshSteamImages(995);
  finish(packet()); await Promise.all([opening, explicit]);
  assert.deepEqual(policies, ["cache_or_fetch", "refresh_if_expired", "force_refresh"]);
});


test("opening details never refreshes permanent Library card artwork or icons", async () => {
  const calls = [];
  globalThis.cacheInvoke = (command, args) => {
    if (command === "cancel_artwork_request") return Promise.resolve();
    calls.push(args);
    return Promise.resolve(packet());
  };
  for (const asset of ["hero_blur", "client_icon", "library_capsule", "library_header", "logo"]) await images.loadSteamImage(request(7991, asset));
  calls.length = 0;
  await images.refreshSteamImagesOnOpen(7991, { requestId: "opening-permanent", origin: "visible", release() {} });
  assert.equal(calls.length, 0);
  await images.refreshSteamImages(7991);
  assert.equal(calls.filter(args => args.policy === "force_refresh").length, 5);
});
