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

test("page transition fades without moving content or adding a blur filter", async () => {
  const motion = await import(await load("src/lib/utils/motion.ts"));
  const node = document.createElement("div");
  for (const progress of [0, 0.25, 0.5, 0.75, 1]) {
    node.style.cssText = motion.pageTransition(node).css(progress, 1 - progress);
    assert.equal(Number(node.style.opacity), progress, "page fade must preserve each opacity frame");
    assert.ok(node.style.transform === "" || node.style.transform === "none", "page fade must not move content");
    assert.ok(node.style.filter === "" || node.style.filter === "none", "page filter must not create a backdrop root");
  }
});

test("frozen dither requests a persistent drawing buffer without a continuous frame loop", async () => {
  let attributes;
  let draws = 0;
  dom.window.HTMLCanvasElement.prototype.getContext = (_type, configuration) => {
    attributes = configuration;
    return new Proxy({}, { get: (_, name) => {
      if (name === "drawArrays") return () => { draws++; };
      if (name === "getExtension") return () => null;
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
  HTMLElement.prototype.animate = (_frames, options) => {
    const animation = { currentTime: 0, onfinish: null, cancel() {}, effect: {}, playState: "running" };
    if (options.duration === 0) queueMicrotask(() => animation.onfinish?.());
    return animation;
  };
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
    assert.equal(motion.pageTransition(document.createElement("div")).duration, 0);
  } finally {
    globalThis.matchMedia = originalMatchMedia;
  }
});

test("native page fades snapshot the outgoing page and update to the latest navigation target", async () => {
  const previousStart = document.startViewTransition;
  const transitions = [];
  document.startViewTransition = update => {
    let finish;
    const finished = new Promise(resolve => { finish = resolve; });
    const transition = { before: document.querySelector("[data-page]")?.dataset.page, update, ready: Promise.resolve(), finished, skipTransition() { this.skipped = true; finish(); } };
    transitions.push(transition);
    return transition;
  };
  const mocks = {};
  const emptyView = dataModule("export default () => {};");
  for (const [feature, view] of [["home", "HomeView"], ["library", "LibraryView"], ["library", "GameDetailView"], ["store", "StoreView"], ["downloads", "DownloadsView"]]) {
    mocks[`src/lib/features/${feature}/${view}.svelte`] = emptyView;
  }
  const { load } = createModuleLoader(mocks);
  (await import(await load("src/lib/services/application-log.ts"))).setApplicationLoggingEnabled(false);
  const navigation = await import(await load("src/lib/stores/navigation.ts"));
  navigation.selectedGameId.set(null);
  navigation.activeSection.set("home");
  const MainContainer = (await import(await load("src/lib/components/layout/MainContainer.svelte"))).default;
  const target = document.createElement("div");
  document.body.append(target);
  const instance = mount(MainContainer, { target });
  try {
    flushSync();
    navigation.activeSection.set("library");
    flushSync();
    assert.equal(transitions.length, 1);
    assert.equal(transitions[0].before, "home", "the old page must be present for the browser snapshot");
    navigation.activeSection.set("store");
    flushSync();
    assert.equal(transitions[0].skipped, true);
    await transitions[0].update();
    assert.equal(target.querySelector("[data-page]").dataset.page, "home", "an obsolete update must not reappear");
    await transitions[1].update();
    assert.equal(target.querySelector("[data-page]").dataset.page, "store");
    assert.equal(target.querySelector("[data-page]").style.opacity, "", "the live page must retain its normal blur backdrop");
    navigation.activeSection.set("library");
    flushSync();
    const pending = transitions.at(-1);
    navigation.activeSection.set("store");
    flushSync();
    assert.equal(pending.skipped, true, "returning to the displayed page must cancel a pending snapshot");
    await pending.update();
    assert.equal(target.querySelector("[data-page]").dataset.page, "store");
  } finally {
    await unmount(instance);
    target.remove();
    document.startViewTransition = previousStart;
  }
});
