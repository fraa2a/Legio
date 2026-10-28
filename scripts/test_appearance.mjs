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

test("onboarding appearance editor reports drafts without saving or clearing the preview on exit", async () => {
  const initial = { theme: "light", appearance: { ...appearance.defaultAppearance(), surfaceOpacity: 27 } };
  settingsStore.set({ data: { theme: "system", appearance: appearance.defaultAppearance() } });
  const writes = [];
  globalThis.themeSave = async value => { writes.push(value); return value; };
  globalThis.themeInvoke = async command => { writes.push(command); };
  const previews = [];
  const component = (await import(await load("src/lib/features/settings/ThemeSettings.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(component, { target, props: { accent: "#ff8800", preview: initial, onPreview: value => { previews.push(value); previewStore.set(value); } } });
  try {
    await settle();
    assert.equal(target.querySelector('input[name="theme"]:checked').value, "light");
    const dark = target.querySelector('input[name="theme"][value="dark"]');
    dark.checked = true; dark.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    assert.equal(previews.at(-1).theme, "dark");
    assert.equal(previews.at(-1).appearance.surfaceOpacity, 27);
    await new Promise(resolve => setTimeout(resolve, 650)); await settle();
    assert.deepEqual(writes, []);
    assert.equal(get(settingsStore).data.theme, "system");
  } finally { await unmount(instance); target.remove(); }
  await settle();
  assert.equal(get(previewStore).theme, "dark");
  assert.deepEqual(writes, []);
});

test("onboarding keeps appearance edits across steps and saves them only on completion", async () => {
  let finishPicker;
  const commands = [];
  globalThis.themeChooseBackground = () => new Promise(resolve => { finishPicker = resolve; });
  globalThis.themeInvoke = async command => { commands.push(command); return command === "import_theme_background" ? "chosen-background" : undefined; };
  const { load: loadSetup } = createModuleLoader({
    ...mocks,
    "@tauri-apps/plugin-dialog": dataModule("export const open = () => globalThis.themeChooseBackground();"),
    "src/lib/i18n": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const language = writable("it"); export const setLanguage = () => {}; export const t = value => value;`),
    "src/lib/stores/app-info": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const appInfo = writable({data:{platform:"windows",trayAvailable:true}});`),
    "src/lib/stores/navigation": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const activeSection = writable("home");`),
    "src/lib/services/dialog": dataModule("export const pickGameDirectory = async () => null;"),
  });
  settingsStore.set({ data: { theme: "system", appearance: appearance.defaultAppearance(), language: "it", onboardingCompleted: false } });
  const saved = [];
  globalThis.themeSave = async value => { saved.push(value); return value; };
  const component = (await import(await loadSetup("src/lib/features/onboarding/OnboardingView.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(component, { target });
  const click = async label => {
    const button = [...target.querySelectorAll("button")].find(button => button.textContent.trim() === label);
    assert.ok(button, label); button.click(); await settle();
  };
  try {
    await settle(); await click("Continua");
    assert.equal(target.querySelector("h1").textContent, "Personalizzazione / Aspetto");
    const group = [...target.querySelectorAll("button[aria-expanded]")].find(button => button.textContent.includes("Pannelli e dialoghi"));
    group.click(); await settle();
    const opacity = target.querySelector("#theme-surfaceOpacity");
    opacity.value = "35"; opacity.dispatchEvent(new Event("input", { bubbles: true })); await settle();
    assert.equal(get(previewStore).appearance.surfaceOpacity, 35);
    const backgrounds = [...target.querySelectorAll("button[aria-expanded]")].find(button => button.textContent.includes("Sfondo e animazioni"));
    backgrounds.click(); await settle(); await click("Scegli uno sfondo");
    for (const label of ["Continua", "Indietro", "Usa impostazioni attuali"]) {
      assert.equal([...target.querySelectorAll("button")].find(button => button.textContent.trim() === label).disabled, true);
    }
    finishPicker("/background.png"); await settle();
    assert.equal(get(previewStore).appearance.background, "chosen-background");
    await click("Continua");
    assert.equal(get(previewStore).appearance.surfaceOpacity, 35);
    await click("Indietro");
    const reopened = [...target.querySelectorAll("button[aria-expanded]")].find(button => button.textContent.includes("Pannelli e dialoghi"));
    reopened.click(); await settle();
    assert.equal(target.querySelector("#theme-surfaceOpacity").value, "35");
    assert.deepEqual(saved, []);
    await click("Continua"); await click("Continua"); await click("Continua");
    assert.equal(target.querySelector("h1").textContent, "Tutto pronto");
    await click("Inizia a giocare");
    assert.equal(saved.length, 1);
    assert.equal(saved[0].appearance.surfaceOpacity, 35);
    assert.equal(saved[0].appearance.background, "chosen-background");
    assert.equal(saved[0].onboardingCompleted, true);
    assert.equal(get(previewStore), null);
  } finally { await unmount(instance); target.remove(); }
  assert.ok(commands.includes("cleanup_theme_backgrounds"));
});

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

test("the accent color previews immediately and resets to the palette accent", async () => {
  const initial = { theme: "dark", appearance: appearance.defaultAppearance() };
  settingsStore.set({ data: initial });
  const previews = [];
  const component = (await import(await load("src/lib/features/settings/ThemeSettings.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  document.documentElement.dataset.theme = "dark";
  const instance = mount(component, { target, props: { accent: "#fafafa", preview: initial, onPreview: value => previews.push(value) } });
  try {
    await settle();
    const input = target.querySelector("#theme-accent");
    assert.equal(input.value, presetData.find((preset) => preset.id === "dark").palette.accent);
    input.value = "#336699"; input.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();
    assert.equal(previews.at(-1).appearance.accent, "#336699");
    const reset = [...target.querySelectorAll("button")].find((button) => button.getAttribute("aria-label") === "Ripristina {0}" && button.closest("div")?.querySelector("#theme-accent"));
    assert.ok(reset, "reset button next to the accent input");
    reset.click(); await settle();
    assert.equal(previews.at(-1).appearance.accent, null);
  } finally { await unmount(instance); target.remove(); }
});

test("new installs start with the glass panel style", () => {
  assert.equal(appearance.panelStyleOf(appearance.defaultAppearance()), "glass");
});

test("panel style presets set the surface values and report a match", async () => {
  const base = appearance.defaultAppearance();
  for (const style of ["flat", "glass", "solid"]) assert.equal(appearance.panelStyleOf({ ...base, ...appearance.panelStyles[style] }), style);
  assert.equal(appearance.panelStyleOf({ ...base, ...appearance.panelStyles.glass, surfaceOpacity: 33 }), null);
  const initial = { theme: "dark", appearance: base };
  settingsStore.set({ data: initial });
  const previews = [];
  const component = (await import(await load("src/lib/features/settings/ThemeSettings.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const instance = mount(component, { target, props: { accent: "#fafafa", preview: initial, onPreview: value => previews.push(value) } });
  try {
    await settle();
    const toggle = [...target.querySelectorAll("button[aria-expanded]")].find((button) => button.textContent.includes("Pannelli e dialoghi"));
    toggle.click(); await settle();
    const solid = target.querySelector('input[name="panel-style"][value="solid"]');
    solid.checked = true; solid.dispatchEvent(new Event("change", { bubbles: true }));
    await settle();
    const { appearance: applied } = previews.at(-1);
    assert.deepEqual({ surfaceOpacity: applied.surfaceOpacity, surfaceBlur: applied.surfaceBlur, dialogOpacity: applied.dialogOpacity, dialogBlur: applied.dialogBlur }, appearance.panelStyles.solid);
    assert.equal(target.querySelector('input[name="panel-style"]:checked').value, "solid");
  } finally { await unmount(instance); target.remove(); }
});

test("tinted palettes recolor the neutral ramp and a custom accent wins", () => {
  const channels = (hex) => [1, 3, 5].map((index) => Number.parseInt(hex.slice(index, index + 2), 16));
  const dark = presetData.find((preset) => preset.id === "dark");
  const bronze = presetData.find((preset) => preset.id === "bronze");
  const sand = presetData.find((preset) => preset.id === "sand");
  const neutral = appearance.paletteVariables(dark);
  assert.equal(neutral["--color-zinc-950"], "#09090b");
  assert.equal(neutral["--color-zinc-50"], "#fafafa");
  const warm = appearance.paletteVariables(bronze);
  const [red, , blue] = channels(warm["--color-zinc-800"]);
  assert.ok(red > blue, "a warm tint leaves the mid neutrals redder than blue");
  assert.notEqual(warm["--color-zinc-900"], neutral["--color-zinc-900"]);
  assert.ok(channels(warm["--color-zinc-50"]).every((value) => value >= 248), "near-white shades stay neutral");
  const paper = appearance.paletteVariables(sand);
  assert.ok(channels(paper["--color-zinc-900"]).every((value) => value < 60));
  assert.equal(warm["--legio-accent"], bronze.palette.accent);
  const custom = appearance.paletteVariables(bronze, "#336699");
  assert.equal(custom["--legio-accent"], "#336699");
  assert.equal(custom["--legio-focus"], "#336699");
  assert.equal(custom["--legio-accent-text"], "#ffffff");
  assert.equal(appearance.resolvedAccent(bronze, { ...appearance.defaultAppearance(), accent: "#336699" }), "#336699");
  assert.equal(appearance.resolvedAccent(bronze, appearance.defaultAppearance()), bronze.palette.accent);
});

test("every palette keeps readable text, muted text and accent buttons", () => {
  const luminance = (hex) => {
    const [r, g, b] = [1, 3, 5].map((index) => Number.parseInt(hex.slice(index, index + 2), 16) / 255)
      .map((value) => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4));
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const contrast = (first, second) => {
    const [high, low] = [luminance(first), luminance(second)].sort((a, b) => b - a);
    return (high + 0.05) / (low + 0.05);
  };
  assert.ok(presetData.length >= 7);
  for (const preset of presetData) {
    const { palette } = preset;
    assert.ok(contrast(palette.text, palette.surface) >= 7, preset.id + " text on surface");
    assert.ok(contrast(palette.muted, palette.surface) >= 4.5, preset.id + " muted on surface");
    assert.ok(contrast(palette.muted, palette.background) >= 3.5, preset.id + " muted on background");
    assert.ok(contrast(palette.accent, appearance.accentText(palette.accent)) >= 4.5, preset.id + " accent button label");
    assert.equal(preset.scheme === "light", luminance(palette.background) > 0.4, preset.id + " scheme matches its background");
  }
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

  assert.equal(target.querySelectorAll('input[name="theme"]').length, presetData.length + 1, "the theme group starts open");
  assert.equal(target.querySelector("#theme-surfaceOpacity"), null, "other groups start collapsed");
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
  slider.value = "60"; slider.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  await unmount(instance); target.remove();
  pending.shift()(); await settle();
  assert.equal(saved.length, 2, "the final appearance draft must be saved after teardown");
  assert.equal(saved[1].appearance.surfaceOpacity, 60);
  pending.shift()(); await settle();
});
