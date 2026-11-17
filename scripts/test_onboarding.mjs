import assert from "node:assert/strict";
import { test } from "node:test";
import { get, writable } from "svelte/store";
import { createModuleLoader, dataModule as data } from "./test_module_loader.mjs";

const store = writable({ data: {} });
globalThis.setupSettings = { subscribe: store.subscribe, set: value => store.set({ data: value }) };
globalThis.setupAppInfo = writable({ data: { platform: "windows" } });
globalThis.setupPreview = writable(null);
globalThis.setupLanguage = writable("en");
globalThis.setupSection = writable("home");
const mocks = {
  "src/lib/services/appearance": data('export const cleanupBackgrounds = () => globalThis.setupInvoke("cleanup_theme_backgrounds");'),
  "@tauri-apps/api/core": data("export const invoke = (...args) => globalThis.setupInvoke(...args);"),
  "@tauri-apps/plugin-dialog": data("export const open = () => Promise.resolve(null); export const save = () => Promise.resolve(null);"),
  "src/lib/i18n": data("export const language = globalThis.setupLanguage; export const setLanguage = value => language.set(value); export const t = value => value;"),
  "src/lib/stores/settings": data("export const settings = globalThis.setupSettings; export const updateSettings = async patch => { let current; settings.subscribe(value => {current = value.data;})(); const saved = await globalThis.setupInvoke(\"save_settings\", {settings:{...current,...patch}}); settings.set(saved); return saved; };"),
  "src/lib/stores/app-info": data("export const appInfo = globalThis.setupAppInfo;"),
  "src/lib/stores/appearance": data("export const appearancePreview = globalThis.setupPreview;"),
  "src/lib/stores/games": data("export const games = { reload: async () => { globalThis.setupReloads = (globalThis.setupReloads ?? 0) + 1; } };"),
  "src/lib/stores/navigation": data("export const activeSection = globalThis.setupSection;"),
};
const { load } = createModuleLoader(mocks);
const setup = await import(await load("src/lib/stores/onboarding.ts"));
const prefs = { onboardingCompleted: false, theme: "system", language: "system", appearance: {}, downloadPath: null, launchOnSystemStart: false, launchMinimized: false, launchInLibrary: false, verifyVerifiedDownloads: true };

test("Windows onboarding previews without writes and retries failed completion", async () => {
  store.set({ data: structuredClone(prefs) });
  const calls = [];
  let fail = true;
  globalThis.setupInvoke = async (command, args) => {
    calls.push(command);
    if (command === "cleanup_theme_backgrounds") return;
    assert.equal(command, "save_settings");
    if (fail) throw new Error("save failed");
    return args.settings;
  };
  await setup.startOnboarding();
  setup.changeOnboarding({ theme: "eggplant", language: "it", launchInLibrary: true });
  assert.equal(calls.length, 0);
  assert.equal(get(globalThis.setupPreview).theme, "eggplant");
  assert.equal(get(store).data.onboardingCompleted, false);
  await setup.completeOnboarding();
  assert.equal(get(store).data.onboardingCompleted, false);
  assert.match(get(setup.onboarding).error, /save failed/);
  fail = false;
  await setup.completeOnboarding();
  assert.equal(get(store).data.onboardingCompleted, true);
  assert.equal(get(globalThis.setupSection), "library");
  setup.endOnboardingPreview();
  assert.equal(get(globalThis.setupPreview), null);
  assert.equal(calls.at(-1), "cleanup_theme_backgrounds");
});

test("Linux saves compatibility before marking onboarding complete", async () => {
  store.set({ data: structuredClone(prefs) });
  globalThis.setupAppInfo.set({ data: { platform: "linux" } });
  const calls = [];
  globalThis.setupInvoke = async (command, args) => {
    calls.push(command);
    if (command === "cleanup_theme_backgrounds") return;
    if (command === "get_compatibility_defaults") return { runnerPath: null, prefixRoot: null };
    if (command === "list_compatibility_runners") return { runners: [], diagnostics: [] };
    if (command === "save_compatibility_defaults") { assert.equal(args.defaults.runnerPath, "/runner"); return args.defaults; }
    if (command === "save_settings") return args.settings;
    throw new Error(command);
  };
  await setup.startOnboarding();
  setup.changeCompatibility({ runnerPath: "/runner" });
  await setup.completeOnboarding();
  assert.deepEqual(calls.slice(-2), ["save_compatibility_defaults", "save_settings"]);
  assert.equal(get(store).data.onboardingCompleted, true);
  setup.endOnboardingPreview();
});

test("using current settings discards appearance edits and cleans unused backgrounds on exit", async () => {
  const initial = { ...structuredClone(prefs), appearance: { background: "saved-background" } };
  store.set({ data: initial });
  globalThis.setupAppInfo.set({ data: { platform: "windows" } });
  const calls = [];
  globalThis.setupInvoke = async (command, args) => {
    calls.push(command);
    if (command === "cleanup_theme_backgrounds") return;
    assert.equal(command, "save_settings");
    return args.settings;
  };
  await setup.startOnboarding();
  setup.changeOnboarding({ appearance: { background: "discarded-background" } });
  await setup.completeOnboarding(true);
  assert.deepEqual(get(store).data.appearance, initial.appearance);
  assert.equal(get(store).data.onboardingCompleted, true);
  setup.endOnboardingPreview();
  assert.deepEqual(calls, ["save_settings", "cleanup_theme_backgrounds"]);
});

test("changelog retains the cached releases while refreshing and when offline", async () => {
  const api = await import(await load("src/lib/stores/changelog.ts"));
  const cached = { releases: [{ version: "1.0.0", publishedAt: "2026-10-04T12:00:00Z", notes: "Fixes", url: "https://github.com/fraa2a/Legio/releases/tag/v1.0.0", prerelease: false }], cachedAt: 0, warning: null };
  let rejectRefresh;
  globalThis.setupInvoke = command => command === "get_changelog" ? Promise.resolve(cached) : new Promise((resolve, reject) => { rejectRefresh = reject; });
  const pending = api.loadChangelog();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(get(api.changelog).data.releases, cached.releases);
  rejectRefresh(new Error("offline"));
  await pending;
  assert.deepEqual(get(api.changelog).data.releases, cached.releases);
  assert.match(get(api.changelog).data.warning, /offline/);
});

const found = (launcher, games) => ({ launcher, status: "found", games, warnings: [] });
const game = (launcher, key, executable = "/games/" + key + ".exe") => ({ key, launcher, title: key, installDir: "/games", executable, kind: "windows", prefix: null, runnerHint: null, storeId: null });

test("launcher import offers only launchers with games and selects per launcher", async () => {
  const api = await import(await load("src/lib/stores/launcher-import.ts"));
  globalThis.setupInvoke = async command => {
    assert.equal(command, "scan_launcher_games");
    return [found("heroic", [game("heroic", "a"), game("heroic", "b"), game("heroic", "c", null)]), found("lutris", []), { launcher: "itch", status: "notInstalled" }, { launcher: "bottles", status: "unreadable", reason: "bad" }];
  };
  await api.scanLaunchers();
  const state = get(api.launcherImport);
  assert.deepEqual(state.groups.map(group => group.launcher), ["heroic"]);
  const [group] = state.groups;
  api.toggleLauncher("heroic");
  assert.deepEqual(get(api.launcherImport).selected, ["a", "b"]);
  assert.deepEqual(api.launcherSelection(group, get(api.launcherImport).selected), { count: 2, total: 2, all: true });
  api.toggleGame("a");
  assert.equal(api.launcherSelection(group, get(api.launcherImport).selected).all, false);
  api.toggleLauncher("heroic");
  assert.deepEqual(get(api.launcherImport).selected.sort(), ["a", "b"]);
  api.toggleLauncher("heroic");
  assert.deepEqual(get(api.launcherImport).selected, []);
  assert.equal(api.isImportable(group.games[2]), false);
  api.resetLauncherImport();
  assert.deepEqual(get(api.launcherImport).groups, []);
});

test("launcher import reports per-game results and keeps failures selected", async () => {
  const api = await import(await load("src/lib/stores/launcher-import.ts"));
  const calls = [];
  globalThis.setupInvoke = async (command, args) => {
    calls.push(command);
    if (command === "scan_launcher_games") return [found("heroic", [game("heroic", "a"), game("heroic", "b"), game("heroic", "c")])];
    assert.deepEqual(args.selection.map(item => item.key), ["a", "b", "c"]);
    return [{ launcher: "heroic", key: "a", title: "a", status: "imported", warning: null }, { launcher: "heroic", key: "b", title: "b", status: "duplicate" }, { launcher: "heroic", key: "c", title: "c", status: "failed", reason: "nope" }];
  };
  globalThis.setupReloads = 0;
  await api.scanLaunchers();
  api.toggleLauncher("heroic");
  await api.runLauncherImport();
  const state = get(api.launcherImport);
  assert.deepEqual(state.results.map(item => item.status), ["imported", "duplicate", "failed"]);
  assert.deepEqual(state.selected, ["c"]);
  assert.equal(state.importing, false);
  assert.equal(globalThis.setupReloads, 1);
  globalThis.setupInvoke = async () => { throw new Error("backend down"); };
  await api.runLauncherImport();
  assert.match(get(api.launcherImport).error, /backend down/);
  assert.equal(get(api.launcherImport).importing, false);
  api.resetLauncherImport();
});

test("a failed launcher scan leaves the import step hidden", async () => {
  const api = await import(await load("src/lib/stores/launcher-import.ts"));
  globalThis.setupInvoke = async () => { throw new Error("scan failed"); };
  const original = console.error;
  console.error = () => {};
  try { await api.scanLaunchers(); } finally { console.error = original; }
  assert.deepEqual(get(api.launcherImport).groups, []);
});
