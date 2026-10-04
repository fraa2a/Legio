import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import ts from "typescript";
import { get, writable } from "svelte/store";

const data = code => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
const store = writable({ data: {} });
globalThis.setupSettings = { subscribe: store.subscribe, set: value => store.set({ data: value }) };
globalThis.setupAppInfo = writable({ data: { platform: "windows" } });
globalThis.setupPreview = writable(null);
globalThis.setupLanguage = writable("en");
globalThis.setupSection = writable("home");
const mocks = {
  "@tauri-apps/api/core": data("export const invoke = (...args) => globalThis.setupInvoke(...args);"),
  "@tauri-apps/plugin-dialog": data("export const open = () => Promise.resolve(null);"),
  "src/lib/i18n": data("export const language = globalThis.setupLanguage; export const setLanguage = value => language.set(value); export const t = value => value;"),
  "src/lib/stores/settings": data("export const settings = globalThis.setupSettings;"),
  "src/lib/stores/app-info": data("export const appInfo = globalThis.setupAppInfo;"),
  "src/lib/stores/appearance": data("export const appearancePreview = globalThis.setupPreview;"),
  "src/lib/stores/navigation": data("export const activeSection = globalThis.setupSection;"),
};
const modules = new Map();
async function load(path) {
  if (mocks[path]) return mocks[path];
  if (modules.has(path)) return modules.get(path);
  let code = ts.transpileModule(await readFile(new URL("../" + path, import.meta.url), "utf8"), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
  for (const match of [...code.matchAll(/from\s*["']([^"']+)["']/g)]) {
    const specifier = match[1];
    const resolved = specifier.startsWith(".") ? new URL(specifier, "file:///" + path).pathname.slice(1) : specifier;
    const url = specifier.startsWith(".") ? await load(mocks[resolved] ? resolved : resolved + ".ts") : mocks[specifier] ?? import.meta.resolve(specifier);
    code = code.replaceAll(JSON.stringify(specifier), JSON.stringify(url));
  }
  const url = data(code); modules.set(path, url); return url;
}
const setup = await import(await load("src/lib/stores/onboarding.ts"));
const prefs = { onboardingCompleted: false, theme: "system", language: "system", appearance: {}, downloadPath: null, launchOnSystemStart: false, launchMinimized: false, launchInLibrary: false, verifyVerifiedDownloads: true };

test("Windows onboarding previews without writes and retries failed completion", async () => {
  store.set({ data: structuredClone(prefs) });
  const calls = [];
  let fail = true;
  globalThis.setupInvoke = async (command, args) => {
    calls.push(command);
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
});

test("Linux saves compatibility before marking onboarding complete", async () => {
  store.set({ data: structuredClone(prefs) });
  globalThis.setupAppInfo.set({ data: { platform: "linux" } });
  const calls = [];
  globalThis.setupInvoke = async (command, args) => {
    calls.push(command);
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

test("news retains the persistent feed while refreshing and when offline", async () => {
  const api = await import(await load("src/lib/stores/news.ts"));
  const cached = { feed: { schemaVersion: 1, generatedAt: "2026-10-04T12:00:00Z", items: [] }, cachedAt: 0, warning: null };
  let rejectRefresh;
  globalThis.setupInvoke = command => command === "get_news" ? Promise.resolve(cached) : new Promise((resolve, reject) => { rejectRefresh = reject; });
  const pending = api.loadNews();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(get(api.news).data.feed, cached.feed);
  rejectRefresh(new Error("offline"));
  await pending;
  assert.deepEqual(get(api.news).data.feed, cached.feed);
  assert.match(get(api.news).data.warning, /offline/);
});
