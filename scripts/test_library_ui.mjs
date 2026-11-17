import assert from "node:assert/strict";
import { test } from "node:test";
import { get, writable } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const defaultView = { mode: "grid", sort: "name", descending: false, coverScale: 100, favoritesOnly: false, playStatus: null };

// library-ui is imported once (identical module URLs are cached), with a settings
// store whose saves resolve or reject on demand.
const settings = writable({ status: "ready", data: {} });
let saves = [];
globalThis.libraryViewFixture = {
  settings,
  updateSettings: (changes) => new Promise((resolve, reject) => saves.push({ changes, resolve: () => { settings.update((state) => ({ ...state, data: { ...state.data, ...changes } })); resolve(); }, reject })),
};
const { load: loadUi } = createModuleLoader({
  "src/lib/stores/settings.ts": dataModule("export const settings = globalThis.libraryViewFixture.settings; export const updateSettings = (changes) => globalThis.libraryViewFixture.updateSettings(changes);"),
});
const libraryUi = await import(await loadUi("src/lib/stores/library-ui.ts"));

async function loadLibraryUi(data) {
  settings.set({ status: "ready", data });
  saves = [];
  return { settings, saves, ...libraryUi };
}

test("library view follows saved settings after startup and subsequent updates", async () => {
  const { settings, libraryPortrait } = await loadLibraryUi({ libraryPortrait: false, libraryView: defaultView });
  assert.equal(get(libraryPortrait), false);
  settings.set({ status: "ready", data: { libraryPortrait: true, libraryView: defaultView } });
  assert.equal(get(libraryPortrait), true);
  settings.set({ status: "ready", data: { libraryPortrait: false, libraryView: defaultView } });
  assert.equal(get(libraryPortrait), false);
});

test("view changes show at once and queued changes build on each other", async () => {
  const { saves, libraryView, updateLibraryView } = await loadLibraryUi({ libraryView: defaultView });
  const first = updateLibraryView({ sort: "playtime", descending: true });
  assert.equal(get(libraryView).sort, "playtime");
  const second = updateLibraryView({ mode: "list" });
  // The second save carries the first change even though it is not saved yet.
  assert.deepEqual(saves[1].changes.libraryView, { ...defaultView, sort: "playtime", descending: true, mode: "list" });
  saves[0].resolve();
  await first;
  assert.equal(get(libraryView).mode, "list", "an older save does not hide a pending change");
  saves[1].resolve();
  await second;
  assert.deepEqual(get(libraryView), { ...defaultView, sort: "playtime", descending: true, mode: "list" });
});

test("cover size previews without saving and a failed save restores the saved view", async () => {
  const { saves, libraryView, previewLibraryView, updateLibraryView } = await loadLibraryUi({ libraryView: defaultView });
  previewLibraryView({ coverScale: 130 });
  assert.equal(get(libraryView).coverScale, 130);
  assert.equal(saves.length, 0);
  const commit = updateLibraryView();
  assert.equal(saves[0].changes.libraryView.coverScale, 130);
  saves[0].reject(new Error("disk full"));
  await assert.rejects(commit, /disk full/);
  assert.equal(get(libraryView).coverScale, 100);
});

test("collections are created, reordered and deleted", async () => {
  const calls = [];
  globalThis.organizationFixture = { calls };
  const organization = { games: [], collections: [{ id: "c1", name: "A", gameIds: [] }, { id: "c2", name: "B", gameIds: [] }] };
  const { load } = createModuleLoader({
    "src/lib/i18n": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const language = writable("it"); export const t = (value) => value;`),
    "src/lib/services/library-organization": dataModule(`
      const state = ${JSON.stringify(organization)};
      const record = (name, ...args) => { globalThis.organizationFixture.calls.push([name, ...args]); return structuredClone(state); };
      export const getLibraryOrganization = async () => structuredClone(state);
      export const setGamesFavorite = async (...args) => record("favorite", ...args);
      export const setGamesPlayStatus = async (...args) => record("status", ...args);
      export const setGamesCollections = async (...args) => record("collections", ...args);
      export const createCollection = async (name) => { state.collections.push({ id: "c3", name, gameIds: [] }); return record("create", name); };
      export const renameCollection = async (...args) => record("rename", ...args);
      export const deleteCollection = async (id) => { state.collections = state.collections.filter((item) => item.id !== id); return record("delete", id); };
      export const reorderCollections = async (ids) => { state.collections = ids.map((id) => state.collections.find((item) => item.id === id)); return record("reorder", ids); };
    `),
  });
  const store = await import(await load("src/lib/stores/library-organization.ts"));
  await store.libraryOrganization.load();
  assert.equal(await store.addCollection("New"), "c3", "the created collection is identified by its new ID");
  await store.moveCollection("c2", -1);
  assert.deepEqual(calls.at(-1), ["reorder", ["c2", "c1", "c3"]]);
  await store.moveCollection("c1", -1);
  assert.deepEqual(calls.at(-1), ["reorder", ["c1", "c2", "c3"]]);
  const before = calls.length;
  await store.moveCollection("c3", 1);
  assert.equal(calls.length, before, "the last collection cannot move further down");
  await store.removeCollection("c2");
  assert.deepEqual(calls.at(-1), ["delete", "c2"]);
  assert.deepEqual(get(store.libraryOrganization).data.collections.map((item) => item.id), ["c1", "c3"]);
  delete globalThis.organizationFixture;
});
