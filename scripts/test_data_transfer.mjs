import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const { load } = createModuleLoader({
  "src/lib/i18n": dataModule("export const t = value => value;"),
  "src/lib/services/data-transfer": dataModule(`
    const backend = globalThis.dataTransferBackend;
    export const exportUserData = async path => backend.exportTo(path);
    export const stageUserDataImport = async path => backend.stage(path);
  `),
  "src/lib/services/database-recovery": dataModule(`
    export const restartApp = async () => { globalThis.dataTransferBackend.restarts++; };
  `),
  "src/lib/services/dialog": dataModule(`
    const backend = globalThis.dataTransferBackend;
    export const pickDataExportPath = async name => backend.savePath(name);
    export const pickDataImportFile = async () => backend.openPath();
  `),
});
const backend = globalThis.dataTransferBackend = {};
const transfer = await import(await load("src/lib/stores/data-transfer.ts"));

function resetBackend() {
  Object.assign(backend, {
    restarts: 0,
    exported: [],
    staged: [],
    savePath: async name => `/exports/${name}`,
    openPath: async () => "/exports/legio-dati.zip",
    exportTo: async path => { backend.exported.push(path); },
    stage: async path => { backend.staged.push(path); },
  });
  transfer.dataTransfer.set({ pending: null, importPath: null, error: null });
}

test("the export file is named after the local date", () => {
  assert.equal(transfer.exportFileName(new Date(2026, 0, 5)), "legio-dati-2026-01-05.zip");
});

test("a cancelled save dialog exports nothing", async () => {
  resetBackend();
  backend.savePath = async () => null;
  assert.equal(await transfer.exportData(), false);
  assert.deepEqual(backend.exported, []);
  assert.equal(get(transfer.dataTransfer).pending, null);
});

test("a failed export keeps the error", async () => {
  resetBackend();
  backend.exportTo = async () => { throw "Could not create the export"; };
  assert.equal(await transfer.exportData(), false);
  assert.equal(get(transfer.dataTransfer).error, "Could not create the export");
  assert.equal(get(transfer.dataTransfer).pending, null);
});

test("a confirmed import is staged and restarts the app", async () => {
  resetBackend();
  await transfer.chooseImportFile();
  assert.equal(get(transfer.dataTransfer).importPath, "/exports/legio-dati.zip");
  await transfer.confirmImport();
  assert.deepEqual(backend.staged, ["/exports/legio-dati.zip"]);
  assert.equal(backend.restarts, 1);
});

test("a rejected import shows the error without restarting", async () => {
  resetBackend();
  backend.stage = async () => { throw "This export uses a newer database version; update Legio first"; };
  await transfer.chooseImportFile();
  await transfer.confirmImport();
  assert.equal(backend.restarts, 0);
  assert.equal(get(transfer.dataTransfer).pending, null);
  assert.equal(get(transfer.dataTransfer).error, "This export uses a newer database version; update Legio first");
  transfer.cancelImport();
  assert.deepEqual(get(transfer.dataTransfer), { pending: null, importPath: null, error: null });
});
