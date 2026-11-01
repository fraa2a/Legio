import assert from "node:assert/strict";
import { test } from "node:test";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const storeImport = `import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))};`;
const i18n = dataModule(`${storeImport} export const language = writable("it"); export const t = (value, _language, values = []) => value.replace(/\\{(\\d+)\\}/g, (_, index) => String(values[Number(index)] ?? ""));`);

const profile = (id, name, installedVersion = null) => ({ id, name, installedVersion });
const candidate = (profiles, activeProfileId = null) => ({ gameId: "g1", name: "Lethal Company", ecosystemGameId: "lethal-company", activeProfileId, profiles });

test("picks the active profile, else the only one", async () => {
  const load = createModuleLoader({ "src/lib/i18n": i18n }).load;
  const model = await import(await load("src/lib/features/modding/mod-link-model.ts"));
  const a = profile("a", "Alpha");
  const b = profile("b", "Beta");
  assert.equal(model.defaultLinkProfileId(candidate([a, b], "b")), "b");
  assert.equal(model.defaultLinkProfileId(candidate([a])), "a");
  assert.equal(model.defaultLinkProfileId(candidate([a, b])), null);
  assert.equal(model.defaultLinkProfileId(candidate([a, b], "gone")), null);
  assert.equal(model.defaultLinkProfileId(candidate([])), null);
  assert.equal(model.linkProfile(candidate([a, b]), "b"), b);
  assert.equal(model.linkProfile(candidate([a, b]), null), null);
  assert.deepEqual(model.linkProfileOptions(candidate([a, b], "b"), "it"), [
    { value: "a", label: "Alpha" },
    { value: "b", label: "Beta (attivo)" },
  ]);
  assert.deepEqual(model.linkGameOptions([candidate([a])]), [{ value: "g1", label: "Lethal Company" }]);
});

test("derives the install state with the running guard first", async () => {
  const load = createModuleLoader({ "src/lib/i18n": i18n }).load;
  const model = await import(await load("src/lib/features/modding/mod-link-model.ts"));
  const same = profile("a", "Alpha", "1.2.3");
  const older = profile("b", "Beta", "1.0.0");
  const empty = profile("c", "Gamma");
  assert.equal(model.linkInstallState(empty, "1.2.3", true, true), "running");
  assert.equal(model.linkInstallState(empty, "1.2.3", false, true), "busy");
  assert.equal(model.linkInstallState(null, "1.2.3", false, false), "no-profile");
  assert.equal(model.linkInstallState(same, "1.2.3", false, false), "installed");
  assert.equal(model.linkInstallState(older, "1.2.3", false, false), "ready");
  assert.equal(model.linkInstallState(empty, "1.2.3", false, false), "ready");
  assert.equal(model.linkInstallNote("ready", empty, "it"), null);
  assert.match(model.linkInstallNote("ready", older, "it"), /1\.0\.0/);
  for (const state of ["running", "busy", "no-profile", "installed"]) assert.ok(model.linkInstallNote(state, null, "it"));
});

test("collects each unsynced community once", async () => {
  const load = createModuleLoader({ "src/lib/i18n": i18n }).load;
  const model = await import(await load("src/lib/features/modding/mod-link-model.ts"));
  const game = (gameId, ecosystemGameId) => ({ gameId, name: gameId, ecosystemGameId });
  assert.deepEqual(model.unsyncedCommunities({ package: null, candidates: [], unsynced: [game("a", "x"), game("b", "x"), game("c", "y")] }), ["x", "y"]);
  assert.deepEqual(model.unsyncedCommunities({ package: null, candidates: [], unsynced: [] }), []);
});

test("syncs missing catalogs once and queues drained links", async () => {
  const calls = [];
  const resolutions = [
    { package: null, candidates: [], unsynced: [{ gameId: "a", name: "A", ecosystemGameId: "x" }] },
    { package: { description: "d", icon: "i" }, candidates: [{ gameId: "a" }], unsynced: [] },
  ];
  let pending = [{ target: { fullName: "O-N", owner: "O", name: "N", version: "1.0.0" }, error: null }];
  globalThis.modLinkEvents = {};
  const { load } = createModuleLoader({
    "src/lib/services/mod-links": dataModule(`
      export const takeModLinks = async () => { const links = globalThis.modLinkPending(); return links; };
      export const resolveModLink = async (...args) => { globalThis.modLinkCalls.push(["resolve", ...args]); return globalThis.modLinkResolutions.shift(); };
      export const observeModLinks = async (callback) => { globalThis.modLinkEvents.callback = callback; return () => {}; };`),
    "src/lib/stores/modding": dataModule(`export const ensureCatalog = async (id) => { globalThis.modLinkCalls.push(["sync", id]); };`),
    "src/lib/stores/toast": dataModule("export const showToast = (message) => globalThis.modLinkCalls.push([\"toast\", message]);"),
    "src/lib/i18n": i18n,
  });
  globalThis.modLinkCalls = calls;
  globalThis.modLinkResolutions = resolutions;
  globalThis.modLinkPending = () => { const links = pending; pending = []; return links; };
  const store = await import(await load("src/lib/stores/mod-links.ts"));
  const { get } = await import("svelte/store");

  const resolved = await store.resolveLink("O-N", "1.0.0");
  assert.deepEqual(calls, [["resolve", "O-N", "1.0.0"], ["sync", "x"], ["resolve", "O-N", "1.0.0"]]);
  assert.equal(resolved.candidates.length, 1);

  await store.startModLinks();
  assert.equal(get(store.currentModLink).target.fullName, "O-N");
  pending = [{ target: null, error: "bad" }];
  globalThis.modLinkEvents.callback();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(get(store.currentModLink).target.fullName, "O-N");
  store.dismissModLink();
  assert.equal(get(store.currentModLink).error, "bad");
  store.dismissModLink();
  assert.equal(get(store.currentModLink), null);
});
