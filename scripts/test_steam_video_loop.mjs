import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { createModuleLoader } from "./test_module_loader.mjs";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
Object.defineProperty(globalThis, "document", { configurable: true, value: dom.window.document });
const frames = new Map();
let frameId = 0;
globalThis.requestAnimationFrame = callback => { frames.set(++frameId, callback); return frameId; };
globalThis.cancelAnimationFrame = id => frames.delete(id);
dom.window.HTMLMediaElement.prototype.pause = function () {};
const originalContext = dom.window.HTMLCanvasElement.prototype.getContext;
dom.window.HTMLCanvasElement.prototype.getContext = () => ({ drawImage() {} });
const { load } = createModuleLoader({});
const { createSteamVideoSurface } = await import(await load("src/lib/features/library/steam-description-video.ts"));

function fixture(frameCallbacks = true) {
  const video = document.createElement("video");
  const callbacks = new Map();
  let id = 0;
  if (frameCallbacks) {
    video.requestVideoFrameCallback = callback => { callbacks.set(++id, callback); return id; };
    video.cancelVideoFrameCallback = value => callbacks.delete(value);
  }
  Object.defineProperties(video, { videoWidth: { value: 600 }, videoHeight: { value: 170 }, readyState: { value: 4, configurable: true }, seeking: { value: false, configurable: true }, paused: { value: false, configurable: true } });
  const surface = createSteamVideoSurface(video);
  document.body.append(surface.element);
  const canvas = surface.element.querySelector("canvas");
  return { video, surface, canvas, callbacks, frame() {
    const queue = frameCallbacks ? callbacks : frames;
    const pending = [...queue.values()]; queue.clear();
    for (const callback of pending) callback(0, {});
  }, cleanup() { surface.destroy(); surface.element.remove(); frames.clear(); } };
}

test("a video loop holds the last frame while the native player seeks", () => {
  const f = fixture();
  try {
    assert.equal(f.canvas.hidden, true, "the native poster remains visible before a decoded frame");
    f.frame();
    assert.equal(f.canvas.hidden, false);
    assert.equal(f.canvas.width, 600);
    assert.equal(f.canvas.height, 170);
    Object.defineProperty(f.video, "seeking", { value: true });
    Object.defineProperty(f.video, "readyState", { value: 1 });
    f.frame();
    assert.equal(f.canvas.hidden, false, "the decoded surface must remain visible over the native player's empty loop frame");
    assert.equal(f.canvas.width, 600, "seeking must not clear the retained canvas");
    Object.defineProperty(f.video, "seeking", { value: false });
    Object.defineProperty(f.video, "readyState", { value: 4 });
    f.frame();
    assert.equal(f.canvas.hidden, false);
    f.surface.destroy();
    assert.equal(f.callbacks.size, 0, "unmounting cancels pending frame callbacks");
  } finally { f.cleanup(); }
});

test("the retained frame is sized to the displayed video, never above its source", () => {
  const f = fixture();
  globalThis.devicePixelRatio = 2;
  try {
    Object.defineProperty(f.video, "clientWidth", { value: 150, configurable: true });
    f.frame();
    assert.equal(f.canvas.width, 300);
    assert.equal(f.canvas.height, 85);
    Object.defineProperty(f.video, "clientWidth", { value: 500, configurable: true });
    f.frame();
    assert.equal(f.canvas.width, 600, "a larger display keeps the source resolution");
    assert.equal(f.canvas.height, 170);
  } finally { delete globalThis.devicePixelRatio; f.cleanup(); }
});

test("older players paint while playing and stop their fallback clock on pause", () => {
  const f = fixture(false);
  try {
    f.video.dispatchEvent(new dom.window.Event("playing"));
    f.frame();
    assert.equal(f.canvas.hidden, false);
    f.video.dispatchEvent(new dom.window.Event("pause"));
    assert.equal(frames.size, 0);
    f.video.dispatchEvent(new dom.window.Event("playing"));
    assert.equal(frames.size, 1);
    f.surface.destroy();
    assert.equal(frames.size, 0);
    f.video.dispatchEvent(new dom.window.Event("playing"));
    assert.equal(frames.size, 0, "destroy removes playback listeners");
  } finally { f.cleanup(); }
});

test("a video far from the viewport drops its source and retained frame until it returns", () => {
  const originalLoad = dom.window.HTMLMediaElement.prototype.load;
  let loads = 0;
  dom.window.HTMLMediaElement.prototype.load = () => { loads++; };
  const video = document.createElement("video");
  video.src = "https://cdn.steamstatic.com/loop.webm";
  const source = document.createElement("source");
  source.src = "https://cdn.steamstatic.com/loop.mp4";
  video.append(source);
  Object.defineProperties(video, { videoWidth: { value: 600 }, videoHeight: { value: 170 }, readyState: { value: 4 }, seeking: { value: false } });
  const callbacks = new Map();
  let callbackId = 0;
  video.requestVideoFrameCallback = callback => { callbacks.set(++callbackId, callback); return callbackId; };
  video.cancelVideoFrameCallback = value => callbacks.delete(value);
  const surface = createSteamVideoSurface(video);
  try {
    const pending = [...callbacks.values()];
    callbacks.clear();
    for (const callback of pending) callback(0, {});
    const canvas = surface.element.querySelector("canvas");
    assert.equal(canvas.width, 600);
    surface.setNear(false);
    assert.equal(video.hasAttribute("src"), false);
    assert.equal(video.children.length, 0);
    assert.equal(loads, 1, "reloading the emptied video releases its decoder");
    assert.equal(canvas.width, 0);
    assert.equal(canvas.hidden, true);
    assert.equal(callbacks.size, 0, "a far video schedules no frame callbacks");
    surface.setNear(false);
    assert.equal(loads, 1, "an unloaded video is not reloaded again");
    surface.setNear(true);
    assert.equal(video.getAttribute("src"), "https://cdn.steamstatic.com/loop.webm");
    assert.equal(video.querySelector("source"), source);
    assert.equal(loads, 2);
    assert.equal(callbacks.size, 1, "a returning video paints again");
  } finally { surface.destroy(); dom.window.HTMLMediaElement.prototype.load = originalLoad; }
});

test("players without a canvas context retain native playback", () => {
  dom.window.HTMLCanvasElement.prototype.getContext = () => null;
  try {
    const f = fixture();
    try {
      assert.equal(f.surface.element.contains(f.video), true);
      assert.equal(f.canvas, null);
      assert.equal(f.callbacks.size, 0);
    } finally { f.cleanup(); }
  } finally { dom.window.HTMLCanvasElement.prototype.getContext = originalContext; }
});
