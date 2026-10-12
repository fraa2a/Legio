import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>", { pretendToBeVisual: true });
for (const key of ["window", "document", "HTMLElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "CustomEvent", "MutationObserver", "getComputedStyle"]) {
  Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
}
globalThis.matchMedia = window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
globalThis.ResizeObserver = class { observe() {} disconnect() {} };
const frames = new Map();
let sequence = 0;
globalThis.requestAnimationFrame = callback => { frames.set(++sequence, callback); return sequence; };
globalThis.cancelAnimationFrame = id => frames.delete(id);
const { mount, unmount, flushSync } = await import("svelte");
const { load } = createModuleLoader({});

function advanceFrames(time) {
  const scheduled = [...frames.values()]; frames.clear();
  for (const callback of scheduled) callback(time);
}

test("foreground fade uses one paint-frame clock without compositor animations", async () => {
  frames.clear();
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const node = document.createElement("div"); document.body.append(node);
  node.animate = () => { throw new Error("Native compositor animations must not be used"); };
  try {
    const incoming = animateForeground(node, "in", 100);
    assert.equal(node.style.opacity, "0"); assert.equal(frames.size, 1);
    advanceFrames(0); advanceFrames(50);
    assert.equal(Number(node.style.opacity), .5);
    advanceFrames(100); await incoming.finished;
    assert.equal(Number(node.style.opacity), 1); assert.equal(frames.size, 0);
    incoming.cancel(); assert.equal(node.style.opacity, "");
    assert.equal(node.style.filter, ""); assert.equal(node.style.transform, "");
  } finally { node.remove(); frames.clear(); }
});

test("paint-frame fade preserves dim foreground and protected glass", async () => {
  frames.clear();
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const root = document.createElement("div"); root.innerHTML = '<div class="legio-panel"><span style="opacity:.45!important">Text</span></div><div class="legio-glass"><b>Other</b></div>';
  document.body.append(root); const text = root.querySelector("span");
  try {
    const outgoing = animateForeground(root, "out", 100);
    assert.equal(frames.size, 1, "all foreground targets share one frame callback");
    advanceFrames(0); advanceFrames(50);
    assert.equal(Number(text.style.opacity), .225);
    assert.equal(root.querySelector(".legio-panel").style.opacity, "");
    advanceFrames(100); await outgoing.finished;
    assert.equal(text.style.opacity, "0");
    root.remove(); outgoing.cancel(); assert.equal(text.style.opacity, "0", "retired layers remain hidden");
  } finally { root.remove(); frames.clear(); }
});

test("paint-frame cancellation restores opacity and stops future callbacks", async () => {
  frames.clear();
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const node = document.createElement("div"); node.style.setProperty("opacity", ".45", "important");document.body.append(node);
  try {
    const outgoing = animateForeground(node, "out", 100);advanceFrames(0);advanceFrames(50);
    outgoing.cancel();await outgoing.finished;
    assert.equal(Number(node.style.opacity), .45);assert.equal(node.style.getPropertyPriority("opacity"), "important");assert.equal(frames.size, 0);
    advanceFrames(100);assert.equal(Number(node.style.opacity), .45);
    const instant = animateForeground(node, "in", 0);await instant.finished;assert.equal(frames.size, 0);
  } finally { node.remove(); frames.clear(); }
});

test("frozen dither requests a persistent drawing buffer without a continuous frame loop", async () => {
  let attributes;
  let draws = 0;
  dom.window.HTMLCanvasElement.prototype.getContext = (_type, configuration) => {
    attributes = configuration;
    return new Proxy({}, { get: (_, name) => {
      if (name === "drawArrays") return () => { draws++; };
      if (name === "getExtension") return () => null;
      if (name === "isContextLost") return () => false;
      if (name === "checkFramebufferStatus") return () => 1;
      if (String(name).toUpperCase() === name) return 1;
      return () => true;
    } });
  };
  const Background = (await import(await load("src/lib/components/layout/DitherBackground.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(Background, { target, props: {
    opacity: 50, accent: "#ffffff", active: true,
    settings: { disableAnimation: true, backgroundColor: "#000000", enableMouseInteraction: false, waveSpeed: 0.05, waveFrequency: 3, waveAmplitude: 0.3, mouseRadius: 1, colorNum: 4, pixelSize: 2 },
  } });
  try {
    flushSync();
    await new Promise(resolve => setImmediate(resolve));
    flushSync();
    for (const [id, callback] of [...frames]) { frames.delete(id); callback(100); }
    assert.ok(draws > 0, "the static background must draw its initial frame");
    assert.equal(attributes.preserveDrawingBuffer, true, "the drawing buffer must survive presentation");
    assert.equal(frames.size, 0, "static backgrounds must not need a continuous loop");
  } finally {
    await unmount(instance);
    target.remove();
  }
});


test("navigation removes previous page content before the new page enters", async () => {
  frames.clear();
  const emptyView = dataModule("export default () => {};");
  const mocks = {};
  for (const [feature, view] of [["home", "HomeView"], ["library", "LibraryView"], ["library", "GameDetailView"], ["store", "StoreView"], ["downloads", "DownloadsView"]]) {
    mocks[`src/lib/features/${feature}/${view}.svelte`] = emptyView;
  }
  const { load } = createModuleLoader(mocks);
  (await import(await load("src/lib/services/application-log.ts"))).setApplicationLoggingEnabled(false);
  const navigation = await import(await load("src/lib/stores/navigation.ts"));
  const MainContainer = (await import(await load("src/lib/components/layout/MainContainer.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(MainContainer, { target });
  const settle = async () => { flushSync(); await new Promise(resolve => setImmediate(resolve)); flushSync(); };
  const finishNavigation = async () => { advanceFrames(0);advanceFrames(105);await settle();advanceFrames(0);advanceFrames(105);await settle(); };
  try {
    await settle();
    navigation.activeSection.set("library");
    await settle();await finishNavigation();
    assert.deepEqual([...target.querySelectorAll("[data-page]")].map(node => node.dataset.page), ["library"]);
    navigation.selectedGameId.set("first");
    await settle();
    navigation.selectedGameId.set("second");
    await settle();await finishNavigation();
    assert.deepEqual([...target.querySelectorAll("[data-page]")].map(node => node.dataset.page), ["library:second"]);
  } finally {
    await unmount(instance);
    target.remove();
    frames.clear();
  }
});


test("page entrance respects reduced motion", async () => {
  const originalMatchMedia = globalThis.matchMedia;
  globalThis.matchMedia = () => ({ matches: true });
  try {
    const motion = await import(await load("src/lib/utils/motion.ts") + "#reduced-motion");
    assert.equal(motion.pageTransitionDuration, 0);
  } finally {
    globalThis.matchMedia = originalMatchMedia;
  }
});


test("foreground_targets_never_contain_backdrop_surface", async () => {
  const { collectForegroundTargets, animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const root = document.createElement("div");
  root.innerHTML = '<section><p>Text</p><div class="legio-panel"><b>Panel text</b><div class="legio-glass"><span>Nested glass text</span></div></div></section><article style="backdrop-filter:blur(6px)"><i>Other text</i></article>';
  document.body.append(root);
  const targets = collectForegroundTargets(root);
  assert.equal(targets.length, 4);
  for (const target of targets) {
    assert.ok(!target.matches(".legio-panel,.legio-glass,[style]"));
    assert.equal(target.querySelector(".legio-panel,.legio-glass,[style]"), null);
    for (const other of targets) if (target !== other) assert.equal(target.contains(other), false);
  }
  try {
    frames.clear(); const outgoing = animateForeground(root, "out", 105);
    advanceFrames(0);advanceFrames(105);await outgoing.finished;
    for (const surface of root.querySelectorAll(".legio-panel,.legio-glass,article")) assert.equal(surface.style.opacity, "");
    outgoing.cancel(); await animateForeground(root, "in", 0).finished;assert.equal(frames.size, 0);
  } finally { root.remove(); frames.clear(); }
});

test("navigation_keeps_sidebar_and_canvas_nodes and rapid_navigation_never_republishes_obsolete_target", async () => {
  const previousStart = document.startViewTransition;
  document.startViewTransition = () => { throw new Error("whole document snapshots must not be used"); };
  frames.clear();
  const mocks = { "src/lib/services/application-log": dataModule('export const logApplicationEvent = () => new Promise(() => {}); export const logNavigationCheckpoint = () => new Promise(() => {});') };
  const emptyView = dataModule("export default () => {};");
  for (const [feature, view] of [["home", "HomeView"], ["library", "LibraryView"], ["library", "GameDetailView"], ["store", "StoreView"], ["downloads", "DownloadsView"]]) mocks[`src/lib/features/${feature}/${view}.svelte`] = emptyView;
  const { load } = createModuleLoader(mocks);
  const navigation = await import(await load("src/lib/stores/navigation.ts"));
  navigation.selectedGameId.set(null); navigation.activeSection.set("home");
  const MainContainer = (await import(await load("src/lib/components/layout/MainContainer.svelte"))).default;
  const target = document.createElement("div");
  const sidebar = document.createElement("aside"); const canvas = document.createElement("canvas");
  target.append(sidebar, canvas); document.body.append(target);
  const instance = mount(MainContainer, { target });
  const settle = async () => { flushSync(); await new Promise(resolve => setImmediate(resolve)); flushSync(); };
  try {
    await settle();
    target.querySelector("[data-page]").innerHTML = '<div class="legio-panel"><span>Outgoing foreground</span></div>';
    navigation.activeSection.set("library"); await settle();
    assert.equal(target.querySelector("[data-page]").dataset.page, "home", "outgoing foreground waits for exit");
    assert.equal(frames.size, 1);
    navigation.activeSection.set("store"); await settle();
    advanceFrames(0); await settle();
    assert.equal(target.querySelector("[data-page]").dataset.page, "home", "obsolete exit cannot publish library");
    advanceFrames(200); await settle();
    assert.equal(target.querySelector("[data-page]").dataset.page, "store", "logging_does_not_gate_page_swap");
    assert.equal(target.querySelector("aside"), sidebar); assert.equal(target.querySelector("canvas"), canvas);
    assert.equal(sidebar.style.opacity, ""); assert.equal(canvas.style.opacity, "");
    assert.equal(sidebar.style.opacity, "");assert.equal(canvas.style.opacity, "");
  } finally { await unmount(instance); target.remove(); document.startViewTransition = previousStart;frames.clear(); }
});
