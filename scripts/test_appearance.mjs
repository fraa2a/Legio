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
  "src/lib/stores/window-activity": dataModule(`import { writable } from "${import.meta.resolve("svelte/store")}"; export const windowActive = writable(true);`),
  "@tauri-apps/api/core": invoke,
  "@tauri-apps/plugin-dialog": dialog,
  "src/lib/i18n": locale,
  "src/lib/utils/errors": errors,
  "src/lib/services/theme-presets.json": dataModule("export default " + JSON.stringify(presetData)),
  "src/lib/stores/settings": dataModule("export const settings = { subscribe: globalThis.themeSettings.subscribe, set: value => globalThis.themeSettings.set({data: value}) }; export const updateSettings = async patch => { let current; settings.subscribe(value => {current = value.data;})(); const saved = await globalThis.themeSave({...current,...patch}); settings.set(saved); return saved; };"),
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
  const dark = presetData.find((preset) => preset.id === "dark");
  const options = { ...appearance.defaultAppearance(), transparent: true, background: "image", backgroundBlur: 12, surfaceBlur: 8, dialogBlur: 16 };
  appearance.applyAppearance(document.documentElement, "system", options, false, false);
  assert.equal(document.documentElement.dataset.theme, "light");
  assert.equal(document.documentElement.dataset.transparent, "false");
  appearance.applyAppearance(document.documentElement, "dark", options, true, true);
  assert.equal(document.documentElement.dataset.theme, "dark");
  assert.equal(document.documentElement.dataset.transparent, "true");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-background"), dark.palette.background);
  assert.equal(document.documentElement.style.getPropertyValue("--legio-image-blur"), "12px");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-surface-blur"), "blur(8px)");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-dialog-blur"), "blur(16px)");
  appearance.applyAppearance(document.documentElement, "dark", options, true, false);
  assert.equal(document.documentElement.dataset.transparent, "false");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-surface-blur"), "blur(8px)");
  assert.equal(document.documentElement.style.getPropertyValue("--legio-dialog-blur"), "blur(16px)");
  assert.equal(appearance.activePalette("system", false).id, "light");
  assert.equal(appearance.activePalette("system", true).id, "dark");
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
  const openGroup = async (title) => {
    const toggle = [...target.querySelectorAll("button[aria-expanded]")].find((button) => button.textContent.includes(title));
    assert.ok(toggle, title + " group toggle");
    toggle.click();
    await settle();
  };

  assert.equal(target.querySelectorAll('input[name="theme"]').length, 0);
  await openGroup("Tema");
  assert.equal(target.querySelectorAll('input[name="theme"]').length, 3);
  const dark = [...target.querySelectorAll('input[name="theme"]')].find((input) => input.value === "dark");
  dark.checked = true;
  dark.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  assert.equal(get(previewStore).theme, "dark");
  assert.equal(saved.length, 0);
  assert.equal(get(settingsStore).data.theme, "system");

  await openGroup("Pannelli e dialoghi");
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

  await openGroup("Trasparenza della finestra");
  const windowToggle = [...target.querySelectorAll("label")].find((label) => label.textContent.includes("su Hyprland"))?.querySelector("input");
  assert.ok(windowToggle, "window transparency toggle");
  windowToggle.checked = true;
  windowToggle.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  assert.equal(blur.disabled, false);
  assert.equal(dialogBlur.disabled, false);
  assert.equal(opacity.disabled, false);
  assert.equal(blur.value, "14");
  assert.equal(dialogBlur.value, "10");
  windowToggle.checked = false;
  windowToggle.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  assert.equal(blur.disabled, false);
  assert.equal(blur.value, "14");
  assert.equal(dialogBlur.value, "10");

  opacity.dispatchEvent(new Event("pointerdown", { bubbles: true }));
  settingsStore.set({ data: { ...initial, language: "it" } });
  await new Promise((resolve) => setTimeout(resolve, 600));
  await settle();
  assert.equal(saved.length, 0);
  window.dispatchEvent(new Event("pointerup"));
  await new Promise((resolve) => setTimeout(resolve, 600));
  await settle();
  assert.equal(saved.length, 1);
  assert.equal(saved[0].theme, "dark");
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
  const wrapperSource = '<script>import AppBackground from "./src/lib/components/layout/AppBackground.svelte"; let { state } = $props();</script><AppBackground id={$state.id} animation="none" animationOpacity={65} accent="#ffffff" />';
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


test("closing appearance settings flushes the final draft after a pending save", async () => {
  settingsStore.set({ data: { theme: "dark", appearance: appearance.defaultAppearance() } });
  const saved = [];
  const pending = [];
  globalThis.themeSave = value => { saved.push(value); return new Promise(resolve => pending.push(() => resolve(value))); };
  globalThis.themeInvoke = async () => {};
  const component = (await import(await load("src/lib/features/settings/ThemeSettings.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(component, { target }); await settle();
  const group = [...target.querySelectorAll("button[aria-expanded]")].find(button => button.textContent.includes("Pannelli e dialoghi"));
  group.click(); await settle();
  const slider = target.querySelector("#theme-surfaceOpacity");
  slider.value = "20"; slider.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise(resolve => setTimeout(resolve, 650)); await settle();
  assert.equal(saved.length, 1);
  slider.value = "45"; slider.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  await unmount(instance); target.remove();
  pending.shift()(); await settle();
  assert.equal(saved.length, 2, "the final appearance draft must be saved after teardown");
  assert.equal(saved[1].appearance.surfaceOpacity, 45);
  pending.shift()(); await settle();
});
