import assert from "node:assert/strict";
import { test } from "node:test";
import { JSDOM } from "jsdom";
import { compile } from "svelte/compiler";
import { createModuleLoader, dataModule } from "./test_module_loader.mjs";
const dom = new JSDOM('<html><body></body></html>', { pretendToBeVisual: true });
const frames = new Map(); let sequence = 0; let resize;
for (const key of ["window", "document", "navigator", "HTMLElement", "HTMLMediaElement", "Element", "Node", "Text", "Comment", "Event", "CustomEvent", "MutationObserver", "getComputedStyle"]) Object.defineProperty(globalThis, key, { configurable: true, value: dom.window[key] });
globalThis.requestAnimationFrame = callback => { frames.set(++sequence, callback); return sequence; };
globalThis.cancelAnimationFrame = id => frames.delete(id);
globalThis.ResizeObserver = class { constructor(callback) { resize = callback; } observe() {} disconnect() {} };
window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
const { load, resolveImports } = createModuleLoader({ "src/lib/services/application-log": dataModule('export const logApplicationEvent = () => {};') });
const configuration = { waveSpeed: .05, waveFrequency: 3, waveAmplitude: .3, waveColor: null, backgroundColor: '#000000', colorNum: 4, pixelSize: 2, disableAnimation: true, enableMouseInteraction: false, mouseRadius: 1 };
function fakeGl() {
  let generation = 1; let next = 0; let draws = 0; let valid = true; let program; let deleted = 0;
  const constants = new Map();
  const resource = () => ({ generation, id: ++next, deleted: false });
  const check = value => { if (value) { assert.equal(value.generation, generation, 'a restored context cannot reuse old resources'); assert.equal(value.deleted, false); } };
  const api = {
    get draws() { return draws; }, get deleted() { return deleted; },
    reset() { generation++; program = undefined; }, invalid() { valid = false; },
    createShader: resource, createProgram: resource, createBuffer: resource, createTexture: resource, createFramebuffer: resource,
    deleteShader(value) { if (value) { value.deleted = true; deleted++; } }, deleteProgram(value) { if (value) { value.deleted = true; deleted++; } }, deleteBuffer(value) { if (value) { value.deleted = true; deleted++; } }, deleteTexture(value) { if (value) { value.deleted = true; deleted++; } }, deleteFramebuffer(value) { if (value) { value.deleted = true; deleted++; } },
    getShaderParameter: () => true, getProgramParameter: () => true, getUniformLocation: () => ({}), getExtension: () => null, getParameter: () => 'WebGL2', isContextLost: () => false,
    useProgram(value) { check(value); program = value; }, bindBuffer(_type, value) { check(value); }, bindTexture(_type, value) { check(value); }, bindFramebuffer(_type, value) { check(value); },
    checkFramebufferStatus: () => valid ? gl.FRAMEBUFFER_COMPLETE : gl.FRAMEBUFFER_INCOMPLETE_ATTACHMENT,
    drawArrays() { check(program); assert.ok(program); assert.equal(valid, true, 'invalid framebuffer cannot be presented'); draws++; },
  };
  const gl = new Proxy(api, { get(target, name) { if (name in target) return Reflect.get(target, name); if (String(name).toUpperCase() === name) { if (!constants.has(name)) constants.set(name, constants.size + 1); return constants.get(name); } return () => {}; } });
  return gl;
}
const advance = () => { const scheduled = [...frames]; frames.clear(); for (const [, callback] of scheduled) callback(100); };
async function fixture(suppliedGl) {
  frames.clear();
  const { createDitherRenderer } = await import(await load('src/lib/components/layout/dither-renderer.ts'));
  const canvas = document.createElement('canvas'); const container = document.createElement('div'); container.append(canvas);
  let width = 640; container.getBoundingClientRect = () => ({ width, height: 480, left: 0, top: 0 });
  const gl = suppliedGl ?? fakeGl(); let attributes; canvas.getContext = (_type, options) => { attributes = options; return gl; };
  const diagnostics = [];
  const renderer = createDitherRenderer(canvas, value => diagnostics.push(value));
  renderer.resize(width, 480);
  const frame = { time: 0, settings: configuration, accent: '#ff0000', mouse: [0,0], ratio: 1 };
  return { renderer, canvas, gl, diagnostics, draw: () => renderer.draw(frame), get attributes() { return attributes; }, resize() { width++; renderer.resize(width, 480); } };
}
test('unrelated_blur_preview_does_not_redraw_frozen_dither and hidden_resize_recovers_without_unbounded_frame_loop', async () => {
  const gl = fakeGl(); let attributes;
  window.HTMLCanvasElement.prototype.getContext = (_type, options) => { attributes = options; return gl; };
  const source = `<script>
    import DitherBackground from "../src/lib/components/layout/DitherBackground.svelte";
    let settings = $state(${JSON.stringify(configuration)}); let active = $state(true); let opacity = $state(50);
  </script><DitherBackground {settings} {active} {opacity} accent="#ff0000" />
  <button onclick={() => { settings = {...settings}; opacity = 60; }}>Preview</button>
  <button onclick={() => active = !active}>Toggle</button>`;
  const Component = (await import(dataModule(await resolveImports(compile(source, { generate: 'client' }).js.code, 'scripts/dither_wrapper.svelte')))).default;
  const { mount, unmount, flushSync } = await import('svelte');
  const target = document.createElement('div'); document.body.append(target);
  const instance = mount(Component, { target });
  try {
    flushSync(); advance(); flushSync(); assert.equal(gl.draws, 2); assert.equal(frames.size, 0);
    target.querySelector('button').click(); flushSync(); advance(); assert.equal(gl.draws, 2); assert.equal(frames.size, 0);
    target.querySelectorAll('button')[1].click(); flushSync();
    const container = target.querySelector('.animated-background'); container.getBoundingClientRect = () => ({ width: 800, height: 500, left: 0, top: 0 }); resize();
    assert.equal(frames.size, 0); assert.equal(gl.draws, 2);
    target.querySelectorAll('button')[1].click(); flushSync(); assert.equal(frames.size, 1); advance();
    assert.equal(gl.draws, 4); assert.equal(frames.size, 0);
    assert.deepEqual(attributes, { alpha: false, antialias: false, preserveDrawingBuffer: true });
  } finally { await unmount(instance); target.remove(); }
});
test('context_restoration_rebuilds_resources_and_draws_valid_frame', async () => {
  const f = await fixture(); try {
    assert.equal(f.draw(), true);
    const event = new window.Event('webglcontextlost', { cancelable: true }); f.canvas.dispatchEvent(event);
    assert.equal(event.defaultPrevented, true); assert.equal(f.draw(), false);
    f.gl.reset(); f.canvas.dispatchEvent(new window.Event('webglcontextrestored'));
    assert.equal(f.draw(), true); assert.equal(f.gl.draws, 4);
    assert.deepEqual(f.diagnostics.map(event => event.event), ['context_lost', 'context_restored']);
  } finally { f.renderer.dispose(); }
});
test('incomplete_framebuffer_keeps_fallback_without_drawing', async () => {
  const f = await fixture(); try {
    f.gl.invalid(); f.resize(); assert.equal(f.draw(), false);
    assert.equal(f.gl.draws, 0); assert.equal(f.diagnostics.at(-1).event, "framebuffer_incomplete");
  } finally { f.renderer.dispose(); assert.ok(f.gl.deleted > 0); }
});

test('partial_allocation_failure_disposes_created_resources', async () => {
  const gl = fakeGl(); gl.createBuffer = () => null;
  const f = await fixture(gl);
  try { assert.equal(f.draw(), false); assert.ok(gl.deleted >= 6); assert.equal(f.diagnostics.at(-1).event, "render_failed"); }
  finally { f.renderer.dispose(); }
});
test('loss_between_passes_never_presents_invalid_frame', async () => {
  const gl = fakeGl(); let lost = false; const draw = gl.drawArrays;
  gl.drawArrays = () => { draw(); lost = true; }; gl.isContextLost = () => lost;
  const f = await fixture(gl);
  try { assert.equal(f.draw(), false); assert.equal(gl.draws, 1); }
  finally { f.renderer.dispose(); }
});
