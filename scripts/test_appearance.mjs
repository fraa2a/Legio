import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { url: "http://localhost" });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLInputElement", "HTMLButtonElement", "HTMLImageElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "MouseEvent", "CustomEvent", "MutationObserver", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
window.matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} });
const { mount, unmount, flushSync } = await import("svelte");
const { get, writable } = await import("svelte/store");
const presetData = JSON.parse(await readFile(new URL("../src/lib/services/theme-presets.json", import.meta.url), "utf8"));
const locale = dataModule('import { writable } from "' + import.meta.resolve("svelte/store") + '"; export const language = writable("it"); export const t = message => message;');
const invoke = dataModule("export const invoke = (...args) => globalThis.themeInvoke(...args);");
const dialog = dataModule("export const open = () => Promise.resolve(null); export const save = () => Promise.resolve(null);");
const errors = dataModule("export const toMessage = error => String(error);");
const settingsStore = writable({ data: {} });
const previewStore = writable(null);
globalThis.themeSettings = settingsStore;
globalThis.themePreview = previewStore;
const mocks = {
  "@tauri-apps/api/core": invoke,
  "@tauri-apps/plugin-dialog": dialog,
  "src/lib/i18n": locale,
  "src/lib/utils/errors": errors,
  "src/lib/services/theme-presets.json": dataModule("export default " + JSON.stringify(presetData)),
  "src/lib/stores/settings": dataModule("export const settings = { subscribe: globalThis.themeSettings.subscribe, set: value => globalThis.themeSettings.set({data: value}) };"),
  "src/lib/stores/appearance": dataModule("export const appearancePreview = globalThis.themePreview;"),
  "src/lib/stores/app-info": dataModule('import { writable } from "' + import.meta.resolve("svelte/store") + '"; export const appInfo = writable({data:{platform:"linux",desktopEnvironment:"hyprland"}});'),
  "src/lib/services/local-state": dataModule("export const saveSettings = value => globalThis.themeSave(value);"),
};

const { load } = createModuleLoader(mocks);
const appearance = await import(await load("src/lib/services/appearance.ts"));
const settle = async () => {
  for (let index = 0; index < 6; index++) { await new Promise((resolve) => setImmediate(resolve)); flushSync(); }
};

test("system palette follows the OS and Hyprland alone can enable window transparency", () => {
  const options = { ...appearance.defaultAppearance(), transparent: true, background: "image", backgroundBlur: 12, surfaceBlur: 8, dialogBlur: 16 };
  appearance.applyAppearance(document.documentElement, "system", options, false, false);
  assert.equal(document.documentElement.dataset.theme, "light");
  assert.equal(document.documentElement.dataset.transparent, "false");
  appearance.applyAppearance(document.documentElement, "eggplant", options, true, true);
  assert.equal(document.documentElement.dataset.theme, "dark");
  assert.equal(document.documentElement.dataset.transparent, "true");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-background"), "#170d20");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-image-blur"), "12px");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-surface-blur"), "8px");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-dialog-blur"), "16px");
  options.surfaceOpacity = 100;
  options.dialogOpacity = 100;
  appearance.applyAppearance(document.documentElement, "eggplant", options, true, true);
  assert.equal(document.documentElement.style.getPropertyValue("--legio-surface-blur"), "0px");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-dialog-blur"), "0px");
  assert.equal(appearance.activePalette("system", options, false).id, "light");
});

test("unreadable editor drafts leave application controls usable", () => {
  const custom = structuredClone(presetData[2]);
  custom.id = "custom";
  custom.palette.text = custom.palette.surface;
  const options = { ...appearance.defaultAppearance(), customThemes: [custom], customThemeId: custom.id };
  assert.equal(appearance.readablePalette(custom), false);
  appearance.applyAppearance(document.documentElement, "custom", options, true, false);
  assert.equal(document.documentElement.style.getPropertyValue("--legio-text"), presetData[0].palette.text);
  assert.equal(appearance.accentText("#ffffff"), "#000000");
  assert.equal(appearance.accentText("#000000"), "#ffffff");
});

test("the theme editor previews before auto save and preserves unrelated settings", async () => {
  const initial = { theme: "system", appearance: appearance.defaultAppearance(), language: "en", steamLibraryPollMinutes: 30 };
  settingsStore.set({ data: initial });
  const saved = [];
  const pendingSaves = [];
  globalThis.themeSave = async (value) => {
    saved.push(value);
    return new Promise((resolve) => pendingSaves.push(() => resolve(value)));
  };
  globalThis.themeInvoke = async (command) => { assert.equal(command, "cleanup_theme_backgrounds"); };
  const component = (await import(await load("src/lib/features/settings/ThemeSettings.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(component, { target });
  await settle();
  const eggplant = [...target.querySelectorAll("label")].find((label) => label.textContent.includes("Palette Viola Melanzana")).querySelector("input");
  eggplant.checked = true;
  eggplant.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  assert.equal(get(previewStore).theme, "eggplant");
  assert.equal(saved.length, 0);
  assert.equal(get(settingsStore).data.theme, "system");
  const opacity = target.querySelector("#theme-surfaceOpacity");
  assert.match(target.querySelector(`label[for="${opacity.id}"]`)?.textContent ?? "", /Opacità/);
  opacity.value = "35";
  opacity.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  assert.equal(get(previewStore).appearance.surfaceOpacity, 35);
  const blur = target.querySelector("#theme-surfaceBlur");
  assert.equal(blur.disabled, false);
  blur.value = "14";
  blur.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  assert.equal(get(previewStore).appearance.surfaceBlur, 14);
  const dialogOpacity = target.querySelector("#theme-dialogOpacity");
  const dialogBlur = target.querySelector("#theme-dialogBlur");
  assert.equal(dialogBlur.disabled, false);
  dialogOpacity.value = "70";
  dialogOpacity.dispatchEvent(new Event("input", { bubbles: true }));
  dialogBlur.value = "10";
  dialogBlur.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  assert.equal(get(previewStore).appearance.dialogOpacity, 70);
  assert.equal(get(previewStore).appearance.dialogBlur, 10);
  dialogOpacity.value = "100";
  dialogOpacity.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  assert.equal(dialogBlur.disabled, true);
  opacity.value = "100";
  opacity.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  assert.equal(blur.disabled, true);
  // Another global setting can change while the appearance editor is open.
  opacity.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  settingsStore.set({ data: { ...initial, language: "it" } });
  await new Promise((resolve) => setTimeout(resolve, 600));
  await settle();
  assert.equal(saved.length, 0);
  window.dispatchEvent(new Event("pointerup"));
  await new Promise((resolve) => setTimeout(resolve, 600));
  await settle();
  assert.equal(saved.length, 1);
  assert.equal(saved[0].theme, "eggplant");
  assert.equal(saved[0].language, "it");
  assert.equal(opacity.disabled, false);
  opacity.value = "42";
  opacity.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  pendingSaves.shift()();
  await settle();
  assert.equal(opacity.value, "42");
  assert.equal(get(previewStore).appearance.surfaceOpacity, 42);
  await new Promise((resolve) => setTimeout(resolve, 600));
  await settle();
  assert.equal(saved.length, 2);
  assert.equal(saved[1].appearance.surfaceOpacity, 42);
  pendingSaves.shift()();
  await settle();
  assert.equal(get(settingsStore).data.appearance.surfaceOpacity, 42);
  await unmount(instance);
  assert.equal(get(previewStore), null);
  target.remove();
});

test("background URL is retained for the same image and revoked when replaced", async () => {
  let requests = 0;
  let sequence = 0;
  const revoked = [];
  URL.createObjectURL = () => "blob:theme-" + ++sequence;
  URL.revokeObjectURL = (url) => revoked.push(url);
  globalThis.themeInvoke = async (command) => {
    assert.equal(command, "get_theme_background");
    requests++;
    return new Uint8Array([1, 2, 3]).buffer;
  };
  const component = (await import(await load("src/lib/components/layout/AppBackground.svelte"))).default;
  const wrapperSource = '<script>import AppBackground from "./src/lib/components/layout/AppBackground.svelte"; let { state } = $props();</script><AppBackground id={$state.id} />';
  let code = compile(wrapperSource, { generate: "client" }).js.code;
  code = code.replace('"./src/lib/components/layout/AppBackground.svelte"', JSON.stringify(await load("src/lib/components/layout/AppBackground.svelte")));
  for (const match of [...code.matchAll(/(?:from|import)\s*["']([^"']+)["']/g)]) {
    if (match[1].startsWith("svelte")) {
      const url = JSON.stringify(import.meta.resolve(match[1]));
      code = code.replaceAll('"' + match[1] + '"', url).replaceAll("'" + match[1] + "'", url);
    }
  }
  const wrapper = (await import(dataModule(code))).default;
  assert.ok(component);
  const state = writable({ id: "first", opacity: 60 });
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(wrapper, { target, props: { state } });
  await settle();
  state.set({ id: "first", opacity: 80 });
  await settle();
  assert.equal(requests, 1);
  assert.equal(revoked.length, 0);
  state.set({ id: "second", opacity: 80 });
  await settle();
  assert.equal(requests, 2);
  assert.deepEqual(revoked, ["blob:theme-1"]);
  await unmount(instance);
  assert.deepEqual(revoked, ["blob:theme-1", "blob:theme-2"]);
  target.remove();
});
