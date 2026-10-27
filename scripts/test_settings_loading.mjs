import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

let readSettings;
let saveSettings;
const { load } = createModuleLoader({
  "src/lib/i18n": dataModule("export const t = value => value;"),
  "src/lib/services/local-state": dataModule("export const getSettings = () => globalThis.settingsLoading.read(); export const saveSettings = value => globalThis.settingsLoading.save(value);"),
  "src/lib/services/appearance": dataModule("export const defaultAppearance = () => ({});"),
  "src/lib/stores/downloads": dataModule("export const installedFolder = { reload: async () => {} };"),
  "src/lib/services/application-log": dataModule("export const setApplicationLoggingEnabled = () => {};"),
});
globalThis.settingsLoading = { read: () => readSettings(), save: value => saveSettings(value) };
const { settings, defaultSettings, updateSettings } = await import(await load("src/lib/stores/settings.ts"));

test("an early preference edit waits for persisted settings instead of overwriting them with defaults", async () => {
  let resolve;
  readSettings = () => new Promise(done => { resolve = done; });
  const writes = [];
  saveSettings = async value => { writes.push(value); return value; };
  const pending = updateSettings({ sidebarCollapsed: false });
  await new Promise(done => setImmediate(done));
  assert.equal(get(settings).status, "loading");
  assert.equal(writes.length, 0);
  resolve({ ...defaultSettings, downloadPath: "/custom/games", theme: "light" });
  await pending;
  assert.equal(writes[0].downloadPath, "/custom/games");
  assert.equal(writes[0].theme, "light");
  assert.equal(writes[0].sidebarCollapsed, false);
});

test("a failed settings read blocks writes and a later retry can save", async () => {
  readSettings = async () => { throw new Error("settings unavailable"); };
  await settings.reload();
  let writes = 0;
  saveSettings = async value => { writes++; return value; };
  await assert.rejects(updateSettings({ theme: "dark" }), /settings unavailable/);
  assert.equal(writes, 0);
  readSettings = async () => ({ ...defaultSettings, downloadPath: "/preserved" });
  await updateSettings({ theme: "dark" });
  assert.equal(writes, 1);
  assert.equal(get(settings).data.downloadPath, "/preserved");
});
