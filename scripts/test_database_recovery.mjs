import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const { load } = createModuleLoader({
  "src/lib/i18n": dataModule("export const t = value => value;"),
  "src/lib/services/database-recovery": dataModule(`
    const backend = globalThis.databaseRecoveryBackend;
    export const getDatabaseFailure = async () => backend.failure;
    export const listDatabaseBackups = async () => backend.backups();
    export const openDataFolder = async () => backend.openFolder();
    export const restoreDatabaseBackup = async fileName => backend.restore(fileName);
    export const resetLocalDatabase = async () => backend.reset();
    export const restartApp = async () => { backend.restarts++; };
  `),
});
const backend = globalThis.databaseRecoveryBackend = { failure: null, restarts: 0 };
const recovery = await import(await load("src/lib/stores/database-recovery.ts"));

function resetBackend() {
  Object.assign(backend, {
    failure: "local database error: file is not a database",
    restarts: 0,
    backups: async () => [
      { fileName: "legio-v38-20.sqlite3", schemaVersion: 38, createdAt: 20 },
      { fileName: "legio-v37-10.sqlite3", schemaVersion: 37, createdAt: 10 },
    ],
    openFolder: async () => {},
    restore: async () => {},
    reset: async () => {},
  });
  recovery.databaseRecovery.set({ backups: [], backupsLoaded: false, selected: "", pending: null, error: null });
}

test("the failure is exposed and the newest backup is selected", async () => {
  resetBackend();
  await recovery.databaseFailure.load();
  assert.equal(get(recovery.databaseFailure).data, "local database error: file is not a database");
  await recovery.loadDatabaseBackups();
  assert.equal(get(recovery.databaseRecovery).selected, "legio-v38-20.sqlite3");
  assert.equal(get(recovery.databaseRecovery).backupsLoaded, true);
});

test("a successful restore of the chosen backup restarts the app", async () => {
  resetBackend();
  const restored = [];
  backend.restore = async fileName => { restored.push(fileName); };
  await recovery.loadDatabaseBackups();
  recovery.selectDatabaseBackup("legio-v37-10.sqlite3");
  await recovery.restoreSelectedBackup();
  assert.deepEqual(restored, ["legio-v37-10.sqlite3"]);
  assert.equal(backend.restarts, 1);
});

test("a failed reset shows the error, keeps the app running and allows a retry", async () => {
  resetBackend();
  backend.reset = async () => { throw "there is no local database file to set aside"; };
  await recovery.startWithNewDatabase();
  assert.equal(backend.restarts, 0);
  assert.equal(get(recovery.databaseRecovery).pending, null);
  assert.equal(get(recovery.databaseRecovery).error, "there is no local database file to set aside");
  backend.reset = async () => {};
  await recovery.startWithNewDatabase();
  assert.equal(backend.restarts, 1);
  assert.equal(get(recovery.databaseRecovery).error, null);
});

test("a second action is ignored while one is running", async () => {
  resetBackend();
  let finish;
  backend.reset = () => new Promise(done => { finish = done; });
  let restores = 0;
  backend.restore = async () => { restores++; };
  await recovery.loadDatabaseBackups();
  const reset = recovery.startWithNewDatabase();
  await recovery.restoreSelectedBackup();
  assert.equal(restores, 0);
  finish();
  await reset;
  assert.equal(backend.restarts, 1);
});

test("without backups there is nothing to restore", async () => {
  resetBackend();
  backend.backups = async () => [];
  await recovery.loadDatabaseBackups();
  await recovery.restoreSelectedBackup();
  assert.equal(get(recovery.databaseRecovery).pending, null);
  assert.equal(backend.restarts, 0);
});
