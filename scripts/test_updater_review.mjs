import assert from "node:assert/strict";
import { test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const loader = createModuleLoader({
  "@tauri-apps/plugin-updater": dataModule("export const check = () => globalThis.updaterReview.check();"),
  "@tauri-apps/plugin-process": dataModule("export const relaunch = async () => {};"),
  "src/lib/services/invoke": dataModule("export const invoke = async () => false;"),
  "src/lib/utils/errors": dataModule("export const toMessage = error => String(error);"),
});
const updater = await import(await loader.load("src/lib/services/app-updater.ts"));
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

test("update checks share demand, close superseded resources and serialize installation", async () => {
  let checks = 0;
  let closes = 0;
  let installs = 0;
  const response = deferred();
  const installation = deferred();
  globalThis.updaterReview = { check: () => { checks++; return response.promise; } };
  const first = updater.checkForAppUpdate();
  const second = updater.checkForAppUpdate();
  await Promise.resolve();
  assert.equal(checks, 1);
  response.resolve({ version: "1", close: async () => { closes++; } });
  await Promise.all([first, second]);
  globalThis.updaterReview.check = async () => ({ version: "2", close: async () => { closes++; }, downloadAndInstall: async () => { installs++; await installation.promise; } });
  await updater.checkForAppUpdate();
  assert.equal(closes, 1);
  const a = updater.installAppUpdate();
  const b = updater.installAppUpdate();
  await Promise.resolve();
  assert.equal(installs, 1);
  await updater.checkForAppUpdate();
  assert.equal(get(updater.updateState).version, "2");
  installation.resolve();
  await Promise.all([a, b]);
  assert.equal(closes, 2, "the installed update resource is also released");
});

test("Windows arguments preserve paths and round trip quoted data", async () => {
  const argsLoader = createModuleLoader({ "src/lib/i18n": dataModule("export const t = s => s;") });
  const { parseLaunchArguments, formatLaunchArguments } = await import(await argsLoader.load("src/lib/utils/launch-arguments.ts"));
  assert.deepEqual(parseLaunchArguments(String.raw`"C:\Games\My Game\data"`), [String.raw`C:\Games\My Game\data`]);
  assert.deepEqual(parseLaunchArguments(String.raw`"\\server\share\My Game"`), [String.raw`\\server\share\My Game`]);
  const values = ["", "C:\\Games\\", 'two "quotes"', "single'quote", String.raw`\\server\share\folder`, "two words"];
  assert.deepEqual(parseLaunchArguments(formatLaunchArguments(values)), values);
});
