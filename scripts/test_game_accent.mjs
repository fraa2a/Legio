import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { get } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const presetData = JSON.parse(await readFile(new URL("../src/lib/services/theme-presets.json", import.meta.url), "utf8"));
const { load } = createModuleLoader({
  "@tauri-apps/api/core": dataModule("export const invoke = async () => null;"),
  "@tauri-apps/plugin-dialog": dataModule("export const open = async () => null; export const save = async () => null;"),
  "src/lib/i18n": dataModule("export const t = value => value;"),
  "src/lib/services/theme-presets.json": dataModule("export default " + JSON.stringify(presetData)),
});
const accent = await import(await load("src/lib/services/game-accent.ts"));
const tint = await import(await load("src/lib/services/color-tint.ts"));
const appearance = await import(await load("src/lib/services/appearance.ts"));

const pixels = (...colors) => Uint8ClampedArray.from(colors.flatMap(([r, g, b, a = 255]) => [r, g, b, a]));
const fill = (count, color) => Array.from({ length: count }, () => color);

test("the dominant color is the most vivid hue and ignores grey, dark and transparent pixels", () => {
  const found = accent.dominantColor(pixels(...fill(40, [200, 30, 40]), ...fill(10, [30, 60, 200]), ...fill(200, [128, 128, 128]), ...fill(200, [0, 0, 0]), ...fill(100, [255, 0, 0, 0])));
  const { hue, chroma } = tint.oklchOf(found);
  assert.ok(hue < 45 || hue > 340, "red wins, got hue " + hue);
  assert.ok(chroma > 0.1);
  assert.equal(accent.dominantColor(pixels(...fill(50, [128, 128, 128]), ...fill(50, [250, 250, 250]))), null);
  assert.equal(accent.dominantColor(pixels(...fill(50, [255, 0, 0, 0]))), null);
  assert.equal(accent.dominantColor(new Uint8ClampedArray(0)), null);
  const blue = accent.dominantColor(pixels(...fill(10, [200, 30, 40]), ...fill(60, [20, 60, 220])));
  assert.ok(tint.oklchOf(blue).hue > 240 && tint.oklchOf(blue).hue < 285);
});

test("an adjusted accent reads as text and focus ring on every palette", () => {
  const samples = ["#000000", "#050505", "#ffffff", "#fafafa", "#808080", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#ff00ff", "#00ffff", "#123456", "#ffe9a8", "#3a2a00", "#7f7fff"];
  for (const preset of presetData) {
    for (const sample of samples) {
      const result = accent.adjustAccent(sample, preset.palette, preset.scheme);
      const label = `${preset.id} ${sample} -> ${result}`;
      assert.match(result, /^#[0-9a-f]{6}$/, label);
      assert.ok(tint.contrastRatio(result, preset.palette.background) >= 4.5, label + " on background");
      assert.ok(tint.contrastRatio(result, preset.palette.surface) >= 4.5, label + " on surface");
      assert.ok(tint.contrastRatio(result, appearance.accentText(result)) >= 4.5, label + " button label");
    }
  }
});

test("adjusting keeps the hue of a saturated color", () => {
  const dark = presetData.find((preset) => preset.id === "dark");
  const light = presetData.find((preset) => preset.id === "light");
  const hue = tint.oklchOf("#2040d0").hue;
  for (const preset of [dark, light]) {
    const result = tint.oklchOf(accent.adjustAccent("#2040d0", preset.palette, preset.scheme));
    assert.ok(Math.abs(result.hue - hue) < 6, preset.id + " hue " + result.hue);
    assert.ok(result.chroma > 0.08, preset.id + " keeps color");
  }
});

test("a sampled color from stale artwork is dropped and navigation clears the old one", async () => {
  const { load: loadStore } = createModuleLoader({
    "src/lib/services/game-accent": dataModule("export const sampleImageColor = image => image.color;"),
  });
  const store = await import(await loadStore("src/lib/stores/game-accent.ts"));
  const image = (color, isConnected = true) => ({ color, isConnected });
  store.publishArtColor("steam:1", "steam:1", image("#aa0000"));
  assert.deepEqual(get(store.gameArtColor), { key: "steam:1", color: "#aa0000" });
  store.keepArtColorFor("steam:2");
  assert.equal(get(store.gameArtColor), null, "moving to B clears A");
  store.publishArtColor("steam:1", "steam:2", image("#aa0000"));
  assert.equal(get(store.gameArtColor), null, "a late load of A is ignored on B");
  store.publishArtColor("steam:2", "steam:2", image("#0000aa", false));
  assert.equal(get(store.gameArtColor), null, "a detached image is ignored");
  store.publishArtColor("steam:2", "steam:2", image("#0000aa"));
  store.keepArtColorFor("steam:2");
  assert.equal(get(store.gameArtColor).color, "#0000aa", "B keeps its own color");
  store.publishArtColor("steam:2", "steam:2", image(null));
  assert.equal(get(store.gameArtColor), null, "artwork without a vivid hue uses the global accent");
  store.publishArtColor("steam:2", "steam:2", image("#0000aa"));
  store.keepArtColorFor(null);
  assert.equal(get(store.gameArtColor), null, "leaving the page clears it");
});
