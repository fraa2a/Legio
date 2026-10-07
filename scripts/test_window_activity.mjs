import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "HTMLElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "MutationObserver"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
let focused = true;
let visible = true;
let minimized = false;
let focusChanged;
let resized;
let removed = 0;
globalThis.activityWindow = {
  isFocused: async () => focused,
  isVisible: async () => visible,
  isMinimized: async () => minimized,
  onFocusChanged: async (listener) => { focusChanged = listener; return () => removed++; },
  onResized: async (listener) => { resized = listener; return () => removed++; },
  listen: async () => () => removed++,
  hide: async () => { visible = false; },
  minimize: async () => { minimized = true; },
};
const { load } = createModuleLoader({
  "@tauri-apps/api/window": dataModule("export const getCurrentWindow = () => globalThis.activityWindow;"),
});
const activity = await import(await load("src/lib/services/window.ts"));
const media = await import(await load("src/lib/services/media-playback.ts"));
const settle = () => new Promise((resolve) => setImmediate(resolve));

test("native focus, minimization and tray visibility suspend and restore activity without polling", async () => {
  const values = [];
  const stop = activity.onWindowActivity((value) => values.push(value));
  await settle();
  assert.equal(values.at(-1), true);
  focused = false;
  focusChanged({ payload: false });
  assert.equal(values.at(-1), false);
  focused = true;
  focusChanged({ payload: true });
  await settle();
  assert.equal(values.at(-1), true);
  await activity.minimizeWindow();
  assert.equal(values.at(-1), false);
  minimized = false;
  resized();
  await settle();
  assert.equal(values.at(-1), true);
  await activity.hideWindow();
  assert.equal(values.at(-1), false);
  visible = true;
  focusChanged({ payload: true });
  await settle();
  assert.equal(values.at(-1), true);
  focusChanged({ payload: true });
  focused = false;
  focusChanged({ payload: false });
  await settle();
  assert.equal(values.at(-1), false, "a late query cannot undo a newer blur");
  stop();
  assert.equal(removed, 3);
});

test("only previously playing media resumes and autoplay while inactive is paused", async () => {
  let change;
  let stopped = false;
  const videos = [document.createElement("video"), document.createElement("video")];
  const states = [{ paused: false, plays: 0 }, { paused: true, plays: 0 }];
  for (let index = 0; index < videos.length; index++) {
    const video = videos[index];
    const state = states[index];
    Object.defineProperty(video, "paused", { get: () => state.paused });
    video.pause = () => { state.paused = true; };
    video.play = async () => { state.paused = false; state.plays++; video.dispatchEvent(new Event("play")); };
    document.body.append(video);
  }
  const stop = media.observeMediaPlayback((listener) => { change = listener; listener(false); return () => { stopped = true; }; });
  assert.equal(states[0].paused, true);
  change(true);
  assert.deepEqual(states.map((state) => state.plays), [1, 0]);
  change(false);
  await videos[1].play();
  assert.equal(states[1].paused, true);
  change(true);
  assert.deepEqual(states.map((state) => state.plays), [2, 2]);
  assert.equal(document.documentElement.dataset.motionPaused, "false");
  stop();
  assert.equal(stopped, true);
  for (const video of videos) video.remove();
});

test("particles cancel frames while inactive and resume with the retained canvas", async () => {
  const { writable } = await import("svelte/store");
  const { mount, unmount, flushSync } = await import("svelte");
  const { compile } = await import("svelte/compiler");
  let draws = 0;
  dom.window.HTMLCanvasElement.prototype.getContext = () => ({
    clearRect() { draws++; }, setTransform() {}, beginPath() {}, moveTo() {}, lineTo() {}, stroke() {}, arc() {}, fill() {},
  });
  window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  globalThis.ResizeObserver = class { observe() {} disconnect() {} };
  const frames = new Map();
  let sequence = 0;
  globalThis.requestAnimationFrame = (callback) => { frames.set(++sequence, callback); return sequence; };
  globalThis.cancelAnimationFrame = (id) => frames.delete(id);
  const source = '<script>import Background from "./src/lib/components/layout/AnimatedBackground.svelte";let { active } = $props();</script><Background opacity={50} color="#ffffff" active={$active}/>';
  const code = await (await createModuleLoader({})).resolveImports(compile(source, { generate: "client" }).js.code, "wrapper.svelte");
  const wrapper = (await import(dataModule(code))).default;
  const active = writable(true);
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(wrapper, { target, props: { active } });
  flushSync();
  await settle();
  flushSync();
  assert.equal(frames.size, 1);
  const canvas = target.querySelector("canvas");
  active.set(false);
  flushSync();
  assert.equal(frames.size, 0);
  const pausedDraws = draws;
  await settle();
  assert.equal(draws, pausedDraws);
  active.set(true);
  flushSync();
  assert.equal(frames.size, 1);
  assert.equal(target.querySelector("canvas"), canvas);
  await unmount(instance);
  assert.equal(frames.size, 0);
  target.remove();
});

test("static dither redraws on changes without keeping a frame loop alive", async () => {
  const { writable } = await import("svelte/store");
  const { mount, unmount, flushSync } = await import("svelte");
  const { compile } = await import("svelte/compiler");
  let draws = 0;
  dom.window.HTMLCanvasElement.prototype.getContext = () => new Proxy({}, { get: (_, name) => {
    if (name === "drawArrays") return () => draws++;
    if (name === "getExtension") return () => null;
    if (String(name).toUpperCase() === name) return 1;
    return () => true;
  } });
  const frames = new Map();
  let sequence = 0;
  globalThis.requestAnimationFrame = (callback) => { frames.set(++sequence, callback); return sequence; };
  globalThis.cancelAnimationFrame = (id) => frames.delete(id);
  const render = () => {
    const entries = [...frames];
    frames.clear();
    for (const [, callback] of entries) callback(100);
  };
  const source = '<script>import Background from "./src/lib/components/layout/DitherBackground.svelte";let { state } = $props();</script><Background opacity={50} accent="#ffffff" active={$state.active} settings={$state.settings}/>';
  const code = await createModuleLoader({}).resolveImports(compile(source, { generate: "client" }).js.code, "wrapper.svelte");
  const wrapper = (await import(dataModule(code))).default;
  const configuration = { disableAnimation: true, backgroundColor: "#000000", enableMouseInteraction: false };
  const state = writable({ active: true, settings: configuration });
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(wrapper, { target, props: { state } });
  flushSync();
  await settle();
  flushSync();
  render();
  assert.equal(draws, 2);
  assert.equal(frames.size, 0);
  state.set({ active: true, settings: { ...configuration, backgroundColor: "#111111" } });
  flushSync();
  render();
  assert.equal(draws, 4);
  state.set({ active: true, settings: { ...configuration, disableAnimation: false } });
  flushSync();
  render();
  assert.equal(frames.size, 1);
  state.set({ active: false, settings: configuration });
  flushSync();
  assert.equal(frames.size, 0);
  await unmount(instance);
  target.remove();
});

test("visible refresh has no timer while inactive and refreshes immediately on focus restore", async () => {
  focused = false;
  visible = true;
  minimized = false;
  const intervals = new Map();
  let nextTimer = 0;
  let refreshes = 0;
  const originalSet = globalThis.setInterval;
  const originalClear = globalThis.clearInterval;
  globalThis.setInterval = (callback) => { const id = ++nextTimer; intervals.set(id, callback); return id; };
  globalThis.clearInterval = (id) => intervals.delete(id);
  let stop;
  try {
    const { startVisibleRefresh } = await import(await load("src/lib/stores/visible-refresh.ts"));
    stop = startVisibleRefresh(() => refreshes++, 30000);
    await settle();
    assert.equal(refreshes, 0);
    assert.equal(intervals.size, 0);
    focused = true;
    focusChanged({ payload: true });
    await settle();
    assert.equal(refreshes, 1);
    assert.equal(intervals.size, 1);
    intervals.values().next().value();
    assert.equal(refreshes, 2);
    focused = false;
    focusChanged({ payload: false });
    assert.equal(intervals.size, 0);
    focused = true;
    focusChanged({ payload: true });
    await settle();
    assert.equal(refreshes, 3);
    assert.equal(intervals.size, 1);
    stop();
    stop = undefined;
    assert.equal(intervals.size, 0);
  } finally {
    stop?.();
    globalThis.setInterval = originalSet;
    globalThis.clearInterval = originalClear;
  }
});
