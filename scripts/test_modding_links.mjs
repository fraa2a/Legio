import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
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

const game = (gameId, ecosystemGameId, packageKnown) => ({ gameId, name: gameId, ecosystemGameId, packageKnown });
const resolution = (unsynced = [], missing = []) => ({ package: null, candidates: [], unsynced, missing });

test("plans one refresh per community, forced only for a known package", async () => {
  const load = createModuleLoader({ "src/lib/i18n": i18n }).load;
  const model = await import(await load("src/lib/features/modding/mod-link-model.ts"));
  const unsynced = (gameId, ecosystemGameId) => ({ gameId, name: gameId, ecosystemGameId });
  assert.deepEqual(model.catalogRefreshes(resolution([unsynced("a", "x"), unsynced("b", "x"), unsynced("c", "y")])), [
    { community: "x", force: false },
    { community: "y", force: false },
  ]);
  assert.deepEqual(model.catalogRefreshes(resolution([], [game("a", "x", true), game("b", "x", true), game("c", "y", false)])), [
    { community: "x", force: true },
    { community: "y", force: false },
  ]);
  assert.deepEqual(model.catalogRefreshes(resolution()), []);
  assert.deepEqual(model.missingVersionNames(resolution([], [game("a", "x", true), game("c", "y", false)])), ["a"]);
});

test("describes who still opens the links after turning the option off", async () => {
  const load = createModuleLoader({ "src/lib/i18n": i18n }).load;
  const model = await import(await load("src/lib/features/modding/mod-link-model.ts"));
  assert.equal(model.linkReleaseNote("nobody", "it"), null);
  assert.ok(model.linkReleaseNote("other", "it"));
  assert.ok(model.linkReleaseNote("legio", "it"));
  assert.notEqual(model.linkReleaseNote("other", "it"), model.linkReleaseNote("legio", "it"));
});

function loadLinkStore() {
  globalThis.modLinkEvents = {};
  return createModuleLoader({
    "src/lib/services/mod-links": dataModule(`
      export const takeModLinks = async () => globalThis.modLinkPending();
      export const resolveModLink = async (...args) => { globalThis.modLinkCalls.push(["resolve", ...args]); return globalThis.modLinkResolutions.shift(); };
      export const getModLinkHandler = async () => { globalThis.modLinkCalls.push(["handler"]); return "other"; };
      export const observeModLinks = async (callback) => { globalThis.modLinkEvents.callback = callback; return () => {}; };`),
    "src/lib/stores/modding": dataModule(`export const refreshCatalog = async (id, force) => { globalThis.modLinkCalls.push(["sync", id, force]); };`),
    "src/lib/stores/settings": dataModule(`export const updateSettings = async (changes) => { globalThis.modLinkCalls.push(["save", changes.thunderstoreLinksEnabled]); };`),
    "src/lib/stores/toast": dataModule(`export const showToast = (message) => globalThis.modLinkCalls.push(["toast", message]);`),
    "src/lib/i18n": i18n,
  }).load;
}

test("refreshes unsynced and outdated catalogs once before resolving again", async () => {
  const calls = [];
  globalThis.modLinkCalls = calls;
  const found = { package: { description: "d", icon: "i" }, candidates: [{ gameId: "a" }], unsynced: [], missing: [] };
  const unsynced = { gameId: "a", name: "A", ecosystemGameId: "x" };
  globalThis.modLinkResolutions = [
    resolution([unsynced], [game("b", "y", true), game("c", "z", false)]),
    found,
    resolution(),
  ];
  const store = await import(await loadLinkStore()("src/lib/stores/mod-links.ts"));

  assert.equal(await store.resolveLink("O-N", "1.0.0"), found);
  assert.deepEqual(calls, [["resolve", "O-N", "1.0.0"], ["sync", "x", false], ["sync", "y", true], ["sync", "z", false], ["resolve", "O-N", "1.0.0"]]);

  calls.length = 0;
  assert.equal((await store.resolveLink("O-N", "1.0.0")).candidates.length, 0);
  assert.deepEqual(calls, [["resolve", "O-N", "1.0.0"]]);
});

test("still reports a version that stays missing after the refresh", async () => {
  const calls = [];
  globalThis.modLinkCalls = calls;
  const missing = resolution([], [game("b", "y", true)]);
  globalThis.modLinkResolutions = [missing, missing];
  const store = await import(await loadLinkStore()("src/lib/stores/mod-links.ts"));
  assert.equal(await store.resolveLink("O-N", "2.0.0"), missing);
  assert.equal(calls.filter(([name]) => name === "sync").length, 1);
});

test("queues drained links", async () => {
  const calls = [];
  globalThis.modLinkCalls = calls;
  let pending = [{ target: { fullName: "O-N", owner: "O", name: "N", version: "1.0.0" }, error: null }];
  globalThis.modLinkPending = () => { const links = pending; pending = []; return links; };
  const store = await import(await loadLinkStore()("src/lib/stores/mod-links.ts"));
  const { get } = await import("svelte/store");

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

test("reports the remaining handler only after turning the option off", async () => {
  const calls = [];
  globalThis.modLinkCalls = calls;
  const store = await import(await loadLinkStore()("src/lib/stores/mod-links.ts"));
  assert.equal(await store.setModLinksEnabled(true), null);
  assert.equal(await store.setModLinksEnabled(false), "other");
  assert.deepEqual(calls, [["save", true], ["save", false], ["handler"]]);
});

test("catalog syncs are shared per community", async () => {
  const source = readFileSync(new URL("../src/lib/stores/modding.ts", import.meta.url), "utf8");
  const imported = source.match(/import \{([^}]*)\} from "\.\.\/services\/modding";/)[1];
  const names = imported.split(",").map((name) => name.trim()).filter((name) => name !== "" && !name.startsWith("type "));
  const gate = { release: null, started: 0 };
  globalThis.moddingGate = gate;
  const overrides = {
    refreshModCatalog: `() => { globalThis.moddingGate.started += 1; return new Promise((resolve) => { globalThis.moddingGate.release = () => resolve({ syncedAt: 5, packageCount: 1 }); }); }`,
    observeCatalogProgress: `async () => () => {}`,
    observeInstallProgress: `async () => () => {}`,
  };
  const service = names.map((name) => `export const ${name} = ${overrides[name] ?? `() => { throw new Error("unexpected ${name}"); }`};`).join("\n");
  const { load } = createModuleLoader({ "src/lib/services/modding": dataModule(service), "src/lib/i18n": i18n });
  const store = await import(await load("src/lib/stores/modding.ts"));
  const { get } = await import("svelte/store");

  const first = store.refreshCatalog("x", false);
  const second = store.refreshCatalog("x", true);
  const waiting = store.ensureCatalog("x");
  assert.equal(gate.started, 1);
  assert.equal(get(store.catalogs).x.refreshing, true);
  let done = false;
  void waiting.then(() => { done = true; });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(done, false);

  gate.release();
  await Promise.all([first, second, waiting]);
  assert.equal(get(store.catalogs).x.refreshing, false);
  assert.equal(get(store.catalogs).x.status.syncedAt, 5);

  await store.ensureCatalog("x");
  assert.equal(gate.started, 1);
  const again = store.refreshCatalog("x", true);
  assert.equal(gate.started, 2);
  gate.release();
  await again;
});
