import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { get, writable } from "svelte/store";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "CustomEvent", "MutationObserver", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
let reduced = false;
const motionListeners = new Set();
globalThis.matchMedia = window.matchMedia = () => ({ get matches() { return reduced; }, addEventListener(_type, listener) { motionListeners.add(listener); }, removeEventListener(_type, listener) { motionListeners.delete(listener); } });
const frames = new Map();
let sequence = 0;
let clock = 0;
globalThis.requestAnimationFrame = (callback) => { frames.set(++sequence, callback); return sequence; };
globalThis.cancelAnimationFrame = (id) => frames.delete(id);
let resizeCallback;
const observers = new Set();
globalThis.ResizeObserver = class { constructor(callback) { resizeCallback = callback; } observe() { observers.add(this); } disconnect() { observers.delete(this); } };
const advance = (step = 1000 / 60) => { clock += step; const scheduled = [...frames.values()]; frames.clear(); for (const callback of scheduled) callback(clock); };

const presetData = JSON.parse(await readFile(new URL("../src/lib/services/theme-presets.json", import.meta.url), "utf8"));
const store = (name, value) => dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const ${name} = writable(${JSON.stringify(value)});`);
const { load, resolveImports } = createModuleLoader({
  "@tauri-apps/api/core": dataModule("export const invoke = async () => null;"),
  "@tauri-apps/plugin-dialog": dataModule("export const open = async () => null; export const save = async () => null;"),
  "src/lib/i18n": dataModule("export const t = value => value;"),
  "src/lib/services/theme-presets.json": dataModule("export default " + JSON.stringify(presetData)),
  "src/lib/services/steam-details": dataModule("export const acquireSteamImage = () => ({ promise: new Promise(() => {}), release() {} });"),
  "src/lib/services/application-log": dataModule("export const logApplicationEvent = () => {};"),
  "src/lib/stores/ambient-game": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const ambientSteamAppId = writable(null);`),
});
const { mount, unmount, flushSync } = await import("svelte");
const loop = await import(await load("src/lib/components/layout/background-loop.ts"));
const starfield = await import(await load("src/lib/components/layout/starfield.ts"));
const seasonal = await import(await load("src/lib/components/layout/seasonal.ts"));
const engine = await import(await load("src/lib/components/layout/particle-engine.ts"));
const grain = await import(await load("src/lib/components/layout/grain.ts"));
const ambient = await import(await load("src/lib/services/ambient-palette.ts"));
const accent = await import(await load("src/lib/services/game-accent.ts"));
const tint = await import(await load("src/lib/services/color-tint.ts"));
const { seededRandom } = await import(await load("src/lib/utils/seeded-random.ts"));

test("seeded star layout is stable and a larger sky only adds stars", () => {
  assert.deepEqual(starfield.starLayout(30), starfield.starLayout(30));
  assert.deepEqual(starfield.starLayout(90).slice(0, 30), starfield.starLayout(30));
  assert.notDeepEqual(starfield.starLayout(30, 1), starfield.starLayout(30, 2));
  const layers = new Set(starfield.starLayout(9).map((star) => star.layer));
  assert.deepEqual([...layers].sort(), [0, 1, 2]);
  for (const star of starfield.starLayout(200)) {
    assert.ok(star.x >= -0.02 && star.x <= 1.02 && star.y >= -0.02 && star.y <= 1.02);
  }
  const random = seededRandom(7);
  assert.equal(random(), seededRandom(7)());
});

test("star count follows density and quality within bounds", () => {
  const medium = starfield.starCount(1920, 1080, "medium", 1);
  assert.ok(starfield.starCount(1920, 1080, "low", 1) < medium && medium < starfield.starCount(1920, 1080, "high", 1));
  assert.ok(starfield.starCount(1920, 1080, "medium", 0.5) < medium);
  assert.ok(starfield.starCount(8000, 8000, "high", 1) <= 900);
  assert.ok(starfield.starCount(10, 10, "low", 1) > 0);
});

test("parallax eases toward the pointer and back to rest without one", () => {
  const moved = starfield.easeParallax({ x: 0, y: 0 }, { x: 1000, y: 500 }, 1000, 500, 200);
  assert.ok(moved.x > 0 && moved.x <= 1 && moved.y > 0);
  const rest = starfield.easeParallax(moved, null, 1000, 500, 10000);
  assert.deepEqual(rest, { x: 0, y: 0 });
});

test("frame-time guard steps down after sustained long frames and ignores stalls", () => {
  let state = loop.initialQuality();
  for (let index = 0; index < 200; index++) state = loop.stepQuality(state, 16.7, 1000 / 60);
  assert.equal(state.level, 0);
  for (let index = 0; index < 200; index++) state = loop.stepQuality(state, 2000, 1000 / 60);
  assert.equal(state.level, 0, "a hidden window is a stall, not a slow frame");
  for (let index = 0; index < 30; index++) state = loop.stepQuality(state, 40, 1000 / 60);
  assert.equal(state.level, 1);
  for (let index = 0; index < 500; index++) state = loop.stepQuality(state, 40, 1000 / 60);
  assert.equal(state.level, loop.qualityLevels.length - 1);
  let capped = loop.initialQuality();
  for (let index = 0; index < 100; index++) capped = loop.stepQuality(capped, 34, 1000 / 30);
  assert.equal(capped.level, 0, "a 30 fps cap is measured against its own interval");
});

test("particles bounce or wrap at the edges", () => {
  const bouncing = { x: 99.9, y: 50, vx: 1, vy: 0 };
  engine.stepParticle(bouncing, 100, 100, "bounce");
  assert.equal(bouncing.vx, -1);
  const falling = { x: 50, y: 105, vx: 0, vy: 10 };
  engine.stepParticle(falling, 100, 100, "wrap", 1, 10);
  assert.ok(falling.y >= -10 && falling.y < 10);
  const seeded = engine.seedParticles(5, 100, 100, seededRandom(1), () => [0, 1]);
  assert.equal(seeded.length, 5);
  assert.ok(seeded.every((particle) => particle.vy === 1));
});

test("seasons fall, wrap and scale their count with quality", () => {
  for (const season of ["snow", "rain", "leaves"]) {
    assert.deepEqual(seasonal.seedSeason(season, 10, 800, 600), seasonal.seedSeason(season, 10, 800, 600));
    const flakes = seasonal.seedSeason(season, 10, 800, 600);
    const before = flakes.map((flake) => flake.y);
    seasonal.stepSeason(flakes, season, 800, 600, 1, 1000 / 60);
    assert.ok(flakes.every((flake, index) => flake.y > before[index] || flake.y < 0), season + " falls");
    for (let step = 0; step < 2000; step++) seasonal.stepSeason(flakes, season, 800, 600, step / 60, 1000 / 60);
    assert.ok(flakes.every((flake) => flake.y >= -24 && flake.y <= 624 && flake.x >= -24 && flake.x <= 824), season + " stays in view");
    assert.ok(seasonal.seasonCount(season, 1920, 1080, 0.5) < seasonal.seasonCount(season, 1920, 1080, 1));
  }
});

const pixels = (...colors) => Uint8ClampedArray.from(colors.flatMap(([r, g, b, a = 255]) => [r, g, b, a]));
const fill = (count, color) => Array.from({ length: count }, () => color);

test("the artwork palette lists distinct hues and keeps the dominant color first", () => {
  const sample = pixels(...fill(40, [200, 30, 40]), ...fill(25, [30, 60, 200]), ...fill(15, [40, 180, 60]), ...fill(100, [128, 128, 128]));
  const palette = accent.dominantPalette(sample);
  assert.equal(palette.length, 3);
  assert.equal(palette[0], accent.dominantColor(sample));
  const hues = palette.map((color) => tint.oklchOf(color).hue);
  assert.ok(hues[0] < 45 || hues[0] > 340, "red first");
  assert.ok(hues[1] > 240 && hues[1] < 285, "blue second");
  assert.equal(accent.dominantPalette(sample, 2).length, 2);
  assert.deepEqual(accent.dominantPalette(pixels(...fill(50, [128, 128, 128]))), []);
  assert.equal(accent.dominantPalette(pixels(...fill(200, [200, 30, 40]), ...fill(1, [30, 60, 200]))).length, 1, "a trace hue is ignored");
});

test("ambient colors keep text and muted text readable on every palette", () => {
  const samples = [[], ["#ff0000"], ["#ffffff", "#000000"], ["#2040d0", "#f0c000", "#00ff80"], ["#ffe9a8"], ["#3a2a00", "#7f7fff"]];
  for (const preset of presetData) {
    const ground = { ...preset.palette };
    for (const sample of samples) {
      const colors = ambient.ambientColors(sample, ground, preset.scheme);
      assert.equal(colors.length, ambient.ambientColorCount);
      for (const color of colors) {
        const label = `${preset.id} ${sample.join(",")} -> ${color}`;
        assert.match(color, /^#[0-9a-f]{6}$/, label);
        for (const ink of [preset.palette.text, preset.palette.muted]) {
          assert.ok(tint.contrastRatio(ink, color) >= Math.min(4.5, tint.contrastRatio(ink, preset.palette.background)), label);
        }
        const offset = tint.oklchOf(color).lightness - tint.oklchOf(preset.palette.background).lightness;
        assert.ok(Math.abs(offset) <= 0.12, label + " stays near the background");
      }
    }
  }
});

test("ambient colors follow the artwork hue and fall back to the accent", () => {
  const dark = presetData.find((preset) => preset.id === "dark");
  const [first] = ambient.ambientColors(["#2040d0"], dark.palette, "dark");
  assert.ok(Math.abs(tint.oklchOf(first).hue - tint.oklchOf("#2040d0").hue) < 10);
  const fallback = ambient.ambientColors([], dark.palette, "dark");
  assert.ok(Math.abs(tint.oklchOf(fallback[0]).hue - tint.oklchOf(dark.palette.accent).hue) < 15);
});

test("color cross-fade starts and ends on its endpoints", () => {
  const from = ["#202040", "#402020"];
  const to = ["#204020", "#101010"];
  assert.deepEqual(ambient.mixColors(from, to, 1), to);
  assert.deepEqual(ambient.mixColors(from, to, 0).map((color) => tint.oklchOf(color).lightness.toFixed(2)), from.map((color) => tint.oklchOf(color).lightness.toFixed(2)));
  const middle = ambient.mixColors(from, to, 0.5)[0];
  const lightness = tint.oklchOf(middle).lightness;
  assert.ok(lightness > Math.min(tint.oklchOf(from[0]).lightness, tint.oklchOf(to[0]).lightness) - 0.01);
});

test("grain noise is deterministic, sparse and balanced", () => {
  const data = grain.grainPixels(32);
  assert.deepEqual(data, grain.grainPixels(32));
  let white = 0; let black = 0; let maxAlpha = 0;
  for (let index = 0; index < data.length; index += 4) {
    if (data[index] === 255) white++; else black++;
    maxAlpha = Math.max(maxAlpha, data[index + 3]);
  }
  assert.ok(Math.abs(white - black) < (white + black) * 0.1);
  assert.ok(maxAlpha <= 40);
});

test("the ambient game is the open game page, the last played game on the home, otherwise none", async () => {
  const { load: loadStore } = createModuleLoader({
    "src/lib/stores/navigation": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const activeSection = writable("home"); export const selectedGameId = writable(null); export const selectedStoreGame = writable(null);`),
    "src/lib/stores/games": store("games", { data: [{ id: "a", steamAppId: 10 }, { id: "b", steamAppId: 20 }, { id: "c", steamAppId: null }] }),
    "src/lib/stores/playtime": dataModule(`import { writable } from ${JSON.stringify(import.meta.resolve("svelte/store"))}; export const playtime = writable({ data: [{ gameId: "a", lastPlayedAt: 5 }, { gameId: "b", lastPlayedAt: 9 }] }); export const recentGameIds = writable({ data: ["b", "a"] });`),
  });
  const navigation = await import(await loadStore("src/lib/stores/navigation"));
  const { ambientSteamAppId } = await import(await loadStore("src/lib/stores/ambient-game.ts"));
  assert.equal(get(ambientSteamAppId), 20, "home shows the last played game");
  navigation.activeSection.set("library");
  assert.equal(get(ambientSteamAppId), null, "the library grid has no game");
  navigation.selectedGameId.set("a");
  assert.equal(get(ambientSteamAppId), 10);
  navigation.selectedGameId.set("c");
  assert.equal(get(ambientSteamAppId), null, "a game without Steam artwork uses the palette");
  navigation.activeSection.set("store");
  navigation.selectedStoreGame.set({ steamAppId: 30, name: "Store" });
  assert.equal(get(ambientSteamAppId), 30);
  navigation.activeSection.set("downloads");
  assert.equal(get(ambientSteamAppId), null);
});

function fakeContext() {
  const calls = new Map();
  const target = {
    get calls() { return calls; },
    createRadialGradient() { count("createRadialGradient"); return { addColorStop() {} }; },
    createImageData(width, height) { return { data: new Uint8ClampedArray(width * height * 4) }; },
  };
  function count(name) { calls.set(name, (calls.get(name) ?? 0) + 1); }
  return new Proxy(target, {
    get(object, name) { if (name in object) return object[name]; return () => count(String(name)); },
    set() { return true; },
  });
}

async function mountBackground(kind, props = {}) {
  const context = fakeContext();
  let contexts = 0;
  window.HTMLCanvasElement.prototype.getContext = () => { contexts++; return context; };
  const source = `<script>
    import CanvasBackground from "../src/lib/components/layout/CanvasBackground.svelte";
    import { createStarfieldRenderer } from "../src/lib/components/layout/starfield";
    import { createSeasonalRenderer } from "../src/lib/components/layout/seasonal";
    import AmbientBackground from "../src/lib/components/layout/AmbientBackground.svelte";
    let { active, kind, palette } = $props();
  </script>
  {#if kind === "starfield"}<CanvasBackground opacity={50} active={$active} config={{ density: "medium", color: "#ffffff" }} create={createStarfieldRenderer} maxFps={30} pointer />
  {:else if kind === "seasonal"}<CanvasBackground opacity={50} active={$active} config={{ season: "snow", color: "#ffffff", scheme: "dark" }} create={createSeasonalRenderer} />
  {:else}<AmbientBackground opacity={50} active={$active} {palette} accent="#c8893a" />{/if}`;
  const code = await resolveImports(compile(source, { generate: "client" }).js.code, "scripts/background_wrapper.svelte");
  const Wrapper = (await import(dataModule(code))).default;
  const active = writable(props.active ?? true);
  const target = document.createElement("div");
  document.body.append(target);
  window.HTMLElement.prototype.getBoundingClientRect = () => ({ width: 800, height: 600, left: 0, top: 0 });
  const instance = mount(Wrapper, { target, props: { active, kind, palette: presetData.find((preset) => preset.id === "dark") } });
  flushSync();
  return { context, target, active, get contexts() { return contexts; }, canvas: target.querySelector("canvas"), unmount: () => { unmount(instance); target.remove(); } };
}

const draws = (context) => (context.calls.get("fillRect") ?? 0) + (context.calls.get("fill") ?? 0);

for (const kind of ["starfield", "seasonal", "ambient"]) {
  test(`${kind} draws while active, stops while paused and releases everything on teardown`, async () => {
    reduced = false; frames.clear();
    const view = await mountBackground(kind);
    try {
      assert.equal(view.contexts, 1);
      for (let index = 0; index < 6; index++) advance();
      assert.ok(draws(view.context) > 0, "frames draw");
      assert.equal(frames.size, 1, "an animated background keeps one frame scheduled");
      view.active.set(false); flushSync();
      assert.equal(frames.size, 0, "a paused background schedules nothing");
      const paused = draws(view.context);
      advance(); advance();
      assert.equal(draws(view.context), paused);
      view.active.set(true); flushSync();
      assert.equal(frames.size, 1, "resuming schedules a frame");
    } finally { view.unmount(); }
    assert.equal(frames.size, 0, "teardown cancels the frame");
    assert.equal(view.canvas.width, 0, "teardown drops the backing store");
    assert.equal(observers.size, 0, "teardown disconnects the resize observer");
    assert.equal(motionListeners.size, 0, "teardown removes the motion listener");
  });

  test(`${kind} draws one frame when it mounts inactive`, async () => {
    reduced = false; frames.clear();
    const view = await mountBackground(kind, { active: false });
    try {
      advance();
      const drawn = draws(view.context);
      assert.ok(drawn > 0, "the first frame draws without focus");
      assert.equal(frames.size, 0, "no further frames are scheduled");
      advance();
      assert.equal(draws(view.context), drawn);
    } finally { view.unmount(); }
  });

  test(`${kind} draws a single frame under reduced motion`, async () => {
    reduced = true; frames.clear();
    const view = await mountBackground(kind);
    try {
      advance();
      const drawn = draws(view.context);
      assert.ok(drawn > 0);
      assert.equal(frames.size, 0, "no further frames are scheduled");
      window.dispatchEvent(new window.MouseEvent("pointermove", { clientX: 10, clientY: 10 }));
      advance();
      assert.equal(draws(view.context), drawn, "the pointer does not move anything");
    } finally { view.unmount(); reduced = false; }
  });
}

test("a lost canvas context stops frames until it is restored", async () => {
  reduced = false; frames.clear();
  const view = await mountBackground("starfield");
  try {
    advance();
    const event = new window.Event("contextlost", { cancelable: true });
    view.canvas.dispatchEvent(event);
    assert.equal(event.defaultPrevented, true);
    assert.equal(frames.size, 0);
    view.canvas.dispatchEvent(new window.Event("contextrestored"));
    assert.equal(frames.size, 1);
    const before = draws(view.context);
    advance();
    assert.ok(draws(view.context) > before);
  } finally { view.unmount(); }
});

test("resizing keeps the canvas within the capped device ratio", async () => {
  reduced = false; frames.clear();
  window.devicePixelRatio = 3;
  const view = await mountBackground("seasonal");
  try {
    assert.equal(view.canvas.width, 1600);
    window.HTMLElement.prototype.getBoundingClientRect = () => ({ width: 400, height: 300, left: 0, top: 0 });
    resizeCallback();
    assert.equal(view.canvas.width, 800);
  } finally { view.unmount(); window.devicePixelRatio = 1; }
});

test("the ambient canvas renders at a fraction of the window and the grain texture is built once", async () => {
  reduced = false; frames.clear();
  const view = await mountBackground("ambient");
  try { assert.equal(view.canvas.width, 100); } finally { view.unmount(); }
  let built = 0;
  window.HTMLCanvasElement.prototype.getContext = () => { built++; return fakeContext(); };
  window.HTMLCanvasElement.prototype.toDataURL = () => "data:image/png;base64,AAAA";
  const Overlay = (await import(await load("src/lib/components/layout/GrainOverlay.svelte"))).default;
  const target = document.createElement("div"); document.body.append(target);
  const first = mount(Overlay, { target, props: { intensity: 40 } });
  const second = mount(Overlay, { target, props: { intensity: 40 } });
  flushSync();
  try {
    assert.equal(built, 1);
    const layer = target.querySelector(".legio-grain");
    assert.equal(layer.style.opacity, "0.4");
    assert.match(layer.style.backgroundImage, /data:image\/png/);
  } finally { unmount(first); unmount(second); target.remove(); }
});
