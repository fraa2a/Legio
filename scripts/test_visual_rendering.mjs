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

test("page foreground fades without moving content or adding a blur filter", async () => {
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const node = document.createElement("div");
  let frames;
  node.animate = values => { frames = values; return { finished: Promise.resolve(), cancel() {} }; };
  await animateForeground(node, "in", 105).finished;
  assert.deepEqual(frames, [{ opacity: 0 }, { opacity: 1 }]);
  await animateForeground(node, "out", 105).finished;
  assert.deepEqual(frames, [{ opacity: 1 }, { opacity: 0 }]);
  assert.equal(node.style.filter, ""); assert.equal(node.style.transform, "");
});

test("outgoing foreground remains hidden when the compositor drops the finished animation", async () => {
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const node = document.createElement("div");
  document.body.append(node);
  let finish;
  node.animate = () => ({ finished: new Promise(resolve => { finish = resolve; }), cancel() {} });
  try {
    const outgoing = animateForeground(node, "out", 105);
    assert.equal(node.style.opacity, "0", "the underlying style must retain the outgoing terminal state");
    finish();
    await outgoing.finished;
    assert.equal(getComputedStyle(node).opacity, "0", "a finished effect must not expose a visible fallback");
    outgoing.cancel();
    assert.equal(node.style.getPropertyValue("opacity"), "", "cancellation must restore the original stylesheet value");
  } finally { node.remove(); }
});

test("foreground cancellation restores inline opacity and priority without brightening dim content", async () => {
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const node = document.createElement("div");
  node.style.setProperty("opacity", "0.45", "important");
  document.body.append(node);
  const effects = [];
  node.animate = keyframes => {
    let reject;
    const effect = { keyframes, finished: new Promise((_resolve, fail) => { reject = fail; }), cancel() { reject(new Error("cancelled")); } };
    effects.push(effect);
    return effect;
  };
  try {
    const outgoing = animateForeground(node, "out", 105);
    assert.deepEqual(effects[0].keyframes, [{ opacity: 0.45 }, { opacity: 0 }]);
    assert.equal(node.style.opacity, "0");
    assert.equal(node.style.getPropertyPriority("opacity"), "", "the animation must be able to override the terminal style");
    outgoing.cancel();
    await outgoing.finished;
    assert.equal(node.style.opacity, "0.45");
    assert.equal(node.style.getPropertyPriority("opacity"), "important");
    const incoming = animateForeground(node, "in", 105);
    assert.deepEqual(effects[1].keyframes, [{ opacity: 0 }, { opacity: 0.45 }]);
    assert.equal(node.style.opacity, "0.45");
    incoming.cancel();
    await incoming.finished;
    assert.equal(node.style.opacity, "0.45");
    assert.equal(node.style.getPropertyPriority("opacity"), "important");
  } finally { node.remove(); }
});

test("foreground animation preserves opacity supplied by a stylesheet", async () => {
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const style = document.createElement("style");
  style.textContent = ".dim-foreground { opacity: 0.45; }";
  const node = document.createElement("div");
  node.className = "dim-foreground";
  document.body.append(style, node);
  let keyframes;
  node.animate = values => { keyframes = values; return { finished: Promise.resolve(), cancel() {} }; };
  try {
    const incoming = animateForeground(node, "in", 105);
    await incoming.finished;
    assert.deepEqual(keyframes, [{ opacity: 0 }, { opacity: 0.45 }]);
    incoming.cancel();
    assert.equal(node.style.opacity, "");
    assert.equal(getComputedStyle(node).opacity, "0.45");
  } finally { node.remove(); style.remove(); }
});

test("retired outgoing foreground never becomes visible during detached-node cleanup", async () => {
  const { animateForeground } = await import(await load("src/lib/utils/foreground-transition.ts"));
  const node = document.createElement("div");
  document.body.append(node);
  node.animate = () => ({ finished: Promise.resolve(), cancel() {} });
  const outgoing = animateForeground(node, "out", 105);
  await outgoing.finished;
  node.remove();
  outgoing.cancel();
  assert.equal(node.style.opacity, "0", "native layer retirement must not receive a visible underlying style");
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
  const previousAnimate = HTMLElement.prototype.animate;
  HTMLElement.prototype.animate = () => ({ finished: Promise.resolve(), cancel() {} });
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
  try {
    await settle();
    navigation.activeSection.set("library");
    await settle();
    assert.deepEqual([...target.querySelectorAll("[data-page]")].map(node => node.dataset.page), ["library"]);
    navigation.selectedGameId.set("first");
    await settle();
    navigation.selectedGameId.set("second");
    await settle();
    assert.deepEqual([...target.querySelectorAll("[data-page]")].map(node => node.dataset.page), ["library:second"]);
  } finally {
    await unmount(instance);
    target.remove();
    HTMLElement.prototype.animate = previousAnimate;
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
  const animated = [];
  const previousAnimate = HTMLElement.prototype.animate;
  HTMLElement.prototype.animate = function (keyframes, options) {
    animated.push({ node: this, keyframes, options });
    return { finished: Promise.resolve(), cancel() {} };
  };
  try {
    await animateForeground(root, "out", 105).finished;
    assert.equal(animated.length, 4);
    assert.ok(animated.every(entry => entry.options.duration === 105));
    for (const surface of root.querySelectorAll(".legio-panel,.legio-glass,article")) {
      assert.equal(surface.style.opacity, "", "protected surfaces must retain their own native opacity");
    }
    const before = animated.length;
    await animateForeground(root, "in", 0).finished;
    assert.equal(animated.length, before, "reduced motion must not animate");
  } finally { root.remove(); HTMLElement.prototype.animate = previousAnimate; }
});

test("navigation_keeps_sidebar_and_canvas_nodes and rapid_navigation_never_republishes_obsolete_target", async () => {
  const previousStart = document.startViewTransition;
  document.startViewTransition = () => { throw new Error("whole document snapshots must not be used"); };
  const previousAnimate = HTMLElement.prototype.animate;
  const animations = [];
  HTMLElement.prototype.animate = function () {
    let finish;
    const animation = { node: this, finished: new Promise(resolve => { finish = resolve; }), cancel() { finish(); }, finish() { finish(); } };
    animations.push(animation); return animation;
  };
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
    assert.equal(animations.length, 1);
    navigation.activeSection.set("store"); await settle();
    animations[0].finish(); await settle();
    assert.equal(target.querySelector("[data-page]").dataset.page, "home", "obsolete exit cannot publish library");
    animations.at(-1).finish(); await settle();
    assert.equal(target.querySelector("[data-page]").dataset.page, "store", "logging_does_not_gate_page_swap");
    assert.equal(target.querySelector("aside"), sidebar); assert.equal(target.querySelector("canvas"), canvas);
    assert.equal(sidebar.style.opacity, ""); assert.equal(canvas.style.opacity, "");
    assert.ok(animations.slice(0, 2).every(animation => animation.node.tagName === "SPAN"));
    assert.ok(animations.every(animation => !animation.node.querySelector(".legio-panel") && animation.node !== sidebar && animation.node !== canvas));
  } finally { await unmount(instance); target.remove(); document.startViewTransition = previousStart; HTMLElement.prototype.animate = previousAnimate; }
});
