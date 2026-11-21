import assert from "node:assert/strict";
import { mock, test } from "node:test";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const loader = createModuleLoader({
  "@tauri-apps/plugin-updater": dataModule("export class Update { constructor(metadata) { Object.assign(this, metadata); }}"),
  "@tauri-apps/plugin-process": dataModule("export const relaunch = async () => {};"),
  "src/lib/services/invoke": dataModule("export const invoke = async command => command === 'check_app_update' ? globalThis.updaterReview.check() : false;"),
  "src/lib/stores/update-notes": dataModule("export const MAX_NOTES_CHARS = 10; export const rememberPendingNotes = async (version, notes) => { globalThis.updaterReview.remembered = [version, notes]; };"),
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
  response.resolve({ update: { version: "1", close: async () => { closes++; } }, unavailable: false });
  await Promise.all([first, second]);
  globalThis.updaterReview.check = async () => ({ update: { version: "2", close: async () => { closes++; }, downloadAndInstall: async () => { installs++; await installation.promise; } }, unavailable: false });
  await updater.checkForAppUpdate();
  assert.equal(closes, 1);
  const a = updater.installAppUpdate();
  const b = updater.installAppUpdate();
  await new Promise(done => setImmediate(done));
  assert.equal(installs, 1);
  await updater.checkForAppUpdate();
  assert.equal(get(updater.updateState).version, "2");
  installation.resolve();
  await Promise.all([a, b]);
  assert.equal(closes, 2, "the installed update resource is also released");
});

test("release notes are trimmed, capped and remembered before installing", async () => {
  globalThis.updaterReview = { check: async () => ({ update: { version: "3", body: "  0123456789abcdef  ", close: async () => {}, downloadAndInstall: async () => {} }, unavailable: false }) };
  await updater.checkForAppUpdate();
  assert.equal(get(updater.updateState).notes, "0123456789");
  await updater.installAppUpdate();
  assert.deepEqual(globalThis.updaterReview.remembered, ["3", "0123456789"]);
  // An emoji across the cap is kept whole instead of leaving a lone surrogate.
  globalThis.updaterReview.check = async () => ({ update: { version: "5", body: "012345678\u{1F600}x", close: async () => {} }, unavailable: false });
  await updater.checkForAppUpdate();
  assert.equal(get(updater.updateState).notes, "012345678\u{1F600}");
  globalThis.updaterReview.check = async () => ({ update: { version: "4", close: async () => {} }, unavailable: false });
  await updater.checkForAppUpdate();
  assert.equal(get(updater.updateState).notes, null);
});

test("periodic checks skip busy ticks and overlapping checks, and stop on teardown", async () => {
  const { startPeriodicChecks, UPDATE_CHECK_INTERVAL_MS } = await import(await loader.load("src/lib/services/update-schedule.ts"));
  mock.timers.enable({ apis: ["setInterval"] });
  try {
    let busy = false;
    let calls = 0;
    let finish;
    const stop = startPeriodicChecks(() => { calls++; return new Promise(done => { finish = done; }); }, () => busy);
    mock.timers.tick(UPDATE_CHECK_INTERVAL_MS - 1);
    assert.equal(calls, 0);
    mock.timers.tick(1);
    assert.equal(calls, 1);
    mock.timers.tick(UPDATE_CHECK_INTERVAL_MS);
    assert.equal(calls, 1, "a running check is not overlapped");
    finish();
    await Promise.resolve();
    busy = true;
    mock.timers.tick(UPDATE_CHECK_INTERVAL_MS);
    assert.equal(calls, 1, "a game or an installation pauses checks");
    busy = false;
    mock.timers.tick(UPDATE_CHECK_INTERVAL_MS);
    assert.equal(calls, 2);
    finish();
    await Promise.resolve();
    stop();
    mock.timers.tick(UPDATE_CHECK_INTERVAL_MS * 2);
    assert.equal(calls, 2);
  } finally {
    mock.timers.reset();
  }
});

test("an installed version is announced once, with its notes when remembered", async () => {
  const state = { appInfo: { data: { version: "2.0.0" } }, settings: { data: { lastSeenVersion: "1.0.0", pendingUpdateNotes: { version: "2.0.0", notes: "Fixes" } } }, saves: [], toasts: [] };
  globalThis.notesReview = state;
  const notesLoader = createModuleLoader({
    "src/lib/i18n": dataModule("export const t = (value, _language, args = []) => value.replace('{0}', args[0]);"),
    "src/lib/stores/app-info": dataModule("export const appInfo = { subscribe: run => { run(globalThis.notesReview.appInfo); return () => {}; } };"),
    "src/lib/stores/settings": dataModule("export const settings = { subscribe: run => { run(globalThis.notesReview.settings); return () => {}; } }; export const updateSettings = async changes => { globalThis.notesReview.saves.push(changes); globalThis.notesReview.settings.data = { ...globalThis.notesReview.settings.data, ...changes }; };"),
    "src/lib/stores/toast": dataModule("export const showToast = (message, tone, action) => globalThis.notesReview.toasts.push([message, tone, action]);"),
  });
  const notes = await import(await notesLoader.load("src/lib/stores/update-notes.ts"));
  await notes.announceInstalledVersion();
  assert.deepEqual(state.saves, [{ lastSeenVersion: "2.0.0", pendingUpdateNotes: null }]);
  assert.equal(state.toasts.length, 1);
  state.toasts[0][2].onRetry();
  assert.deepEqual(get(notes.openedNotes), { version: "2.0.0", notes: "Fixes" });
  await notes.announceInstalledVersion();
  assert.equal(state.saves.length, 1, "the same version is not announced twice");
  state.settings.data.lastSeenVersion = null;
  await notes.announceInstalledVersion();
  assert.equal(state.toasts.length, 1, "the first recorded version is silent");
});

test("Windows arguments preserve paths and round trip quoted data", async () => {
  const argsLoader = createModuleLoader({ "src/lib/i18n": dataModule("export const t = s => s;") });
  const { parseLaunchArguments, formatLaunchArguments } = await import(await argsLoader.load("src/lib/utils/launch-arguments.ts"));
  assert.deepEqual(parseLaunchArguments(String.raw`"C:\Games\My Game\data"`), [String.raw`C:\Games\My Game\data`]);
  assert.deepEqual(parseLaunchArguments(String.raw`"\\server\share\My Game"`), [String.raw`\\server\share\My Game`]);
  const values = ["", "C:\\Games\\", 'two "quotes"', "single'quote", String.raw`\\server\share\folder`, "two words"];
  assert.deepEqual(parseLaunchArguments(formatLaunchArguments(values)), values);
});
