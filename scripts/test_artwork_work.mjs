import assert from "node:assert/strict";
import { test } from "node:test";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";
const { load } = createModuleLoader({ "src/lib/i18n": dataModule("export const t = s => s;"), "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.workInvoke(...args);") });
test("releasing a consumer cancels exactly once", async () => {
  const calls = []; globalThis.workInvoke = async (...args) => { calls.push(args); };
  const { createArtworkWork } = await import(await load("src/lib/services/artwork-work.ts"));
  const work = createArtworkWork(42, "visible"); work.release(); work.release();
  assert.deepEqual(calls, [["cancel_artwork_request", { requestId: work.requestId }]]);
});
let fixtureId = 0;
const selectionFixture = async invoke => {
  const id = ++fixtureId;
  const { writable } = await import("svelte/store");
  globalThis.selectionGames = writable({ data: [{ id: "library-game", steamAppId: 200 }] });
  globalThis.selectionStore = writable(null); globalThis.selectionLibrary = writable(null); globalThis.selectionSection = writable("store");
  globalThis.workInvoke = invoke;
  const loader = createModuleLoader({
    "@tauri-apps/api/core": dataModule("export const invoke = (...args) => globalThis.workInvoke(...args);"),
    "src/lib/stores/games.ts": dataModule(`export const games = globalThis.selectionGames; // ${id}`),
    "src/lib/stores/navigation.ts": dataModule(`export const selectedStoreGame = globalThis.selectionStore; export const selectedGameId = globalThis.selectionLibrary; export const activeSection = globalThis.selectionSection; // ${id}`),
  });
  return import(await loader.load("src/lib/services/artwork-work.ts"));
};
const settle = () => new Promise(resolve => setImmediate(resolve));
test("selection_is_declared_before_page_mount", async () => {
  const calls = []; const service = await selectionFixture(async (command, args) => { if (command === "set_artwork_game") calls.push(args.steamAppId); });
  const stop = service.startArtworkSelectionTracking(); await settle();
  globalThis.selectionStore.set({ steamAppId: 100, name: "Selected game" }); await settle();
  assert.deepEqual(calls, [null, 100]);
  globalThis.selectionSection.set("library"); globalThis.selectionLibrary.set("library-game"); await settle();
  assert.equal(calls.at(-1), 200); stop(); await settle(); assert.equal(calls.at(-1), null);
});
test("rapid_selection_delivers_latest_native_target", async () => {
  const calls = []; const acknowledgements = [];
  const service = await selectionFixture((command, args) => {
    if (command !== "set_artwork_game") return Promise.resolve();
    calls.push(args.steamAppId); return new Promise(resolve => acknowledgements.push(resolve));
  });
  const stop = service.startArtworkSelectionTracking(); acknowledgements.shift()(); await settle();
  globalThis.selectionStore.set({ steamAppId: 100 }); globalThis.selectionStore.set({ steamAppId: 101 }); globalThis.selectionStore.set({ steamAppId: 102 });
  assert.deepEqual(calls, [null, 100]); acknowledgements.shift()(); await settle(); assert.deepEqual(calls, [null, 100, 102]);
  acknowledgements.shift()(); await settle(); stop(); acknowledgements.shift()(); await settle();
});
test("continuous visibility releases artwork demand when a tile leaves the viewport", async () => {
  let notify; let disconnected = 0;
  globalThis.IntersectionObserver = class { constructor(callback) { notify = callback; } observe() {} disconnect() { disconnected++; } };
  try {
    const { observeVisibility } = await import(await load("src/lib/utils/visibility.ts"));
    const states = []; const stop = observeVisibility({}, visible => states.push(visible), false);
    notify([{ isIntersecting: true }]); notify([{ isIntersecting: false }]);
    assert.deepEqual(states, [true, false]); assert.equal(disconnected, 0); stop(); assert.equal(disconnected, 1);
  } finally { delete globalThis.IntersectionObserver; }
});
