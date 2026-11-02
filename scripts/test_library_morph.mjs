import assert from "node:assert/strict";
import { test } from "node:test";
import {
  anchorIndex,
  cubicBezier,
  isNearViewport,
  motionEasing,
  needsMorph,
  planMorph,
} from "../src/lib/features/library/morph-plan.ts";

const portrait = { x: 0, y: 100, width: 176, height: 264 };
const landscape = { x: 192, y: 40, width: 288, height: 134.5 };
const ease = cubicBezier(...motionEasing);

test("easing starts at 0, ends at 1 and rises monotonically", () => {
  assert.equal(ease(0), 0);
  assert.equal(ease(1), 1);
  let previous = 0;
  for (let t = 0.05; t <= 1; t += 0.05) {
    const value = ease(t);
    assert.ok(value >= previous);
    previous = value;
  }
  assert.ok(ease(0.2) > 0.5);
});

test("frames start on the old box and end on the new one", () => {
  const frames = planMorph(portrait, landscape, 16, 8, ease);
  assert.equal(frames.length, 9);
  const [first] = frames;
  const last = frames[frames.length - 1];
  assert.deepEqual(first.card, {
    x: portrait.x - landscape.x,
    y: portrait.y - landscape.y,
    scaleX: portrait.width / landscape.width,
    scaleY: portrait.height / landscape.height,
  });
  assert.deepEqual(last.card, { x: 0, y: 0, scaleX: 1, scaleY: 1 });
  assert.deepEqual(last.artwork, { x: 0, y: 0, scaleX: 1, scaleY: 1 });
  assert.equal(first.ghostOpacity, 1);
  assert.equal(last.ghostOpacity, 0);
  assert.equal(first.contentOpacity, 0);
  assert.equal(last.contentOpacity, 1);
});

test("the old artwork is unscaled at the start", () => {
  const [first] = planMorph(portrait, landscape, 16, 8, ease);
  assert.ok(Math.abs(first.ghost.scaleX * first.card.scaleX - 1) < 1e-9);
  assert.ok(Math.abs(first.ghost.scaleY * first.card.scaleY - 1) < 1e-9);
  assert.ok(Math.abs(first.ghost.x) < 1e-9);
});

test("counter-scaling keeps content and corner radii undistorted on screen", () => {
  for (const frame of planMorph(portrait, landscape, 16, 12, ease)) {
    assert.ok(Math.abs(frame.content.scaleX * frame.card.scaleX - 1) < 1e-9);
    assert.ok(Math.abs(frame.content.scaleY * frame.card.scaleY - 1) < 1e-9);
    assert.ok(Math.abs(frame.radiusX * frame.card.scaleX - 16) < 1e-9);
    assert.ok(Math.abs(frame.radiusY * frame.card.scaleY - 16) < 1e-9);
    // Uniform on screen: artwork and ghost never stretch.
    assert.ok(Math.abs(frame.artwork.scaleX * frame.card.scaleX - frame.artwork.scaleY * frame.card.scaleY) < 1e-9);
    assert.ok(Math.abs(frame.ghost.scaleX * frame.card.scaleX - frame.ghost.scaleY * frame.card.scaleY) < 1e-9);
  }
});

test("artwork always covers the frame", () => {
  for (const frame of planMorph(landscape, portrait, 16, 12, ease)) {
    const coverX = frame.artwork.scaleX * frame.card.scaleX * portrait.width;
    const coverY = frame.artwork.scaleY * frame.card.scaleY * portrait.height;
    const frameWidth = frame.card.scaleX * portrait.width;
    const frameHeight = frame.card.scaleY * portrait.height;
    assert.ok(coverX >= frameWidth - 1e-6);
    assert.ok(coverY >= frameHeight - 1e-6);
  }
});

test("unchanged boxes need no morph", () => {
  assert.equal(needsMorph(portrait, { ...portrait, x: portrait.x + 0.3 }), false);
  assert.equal(needsMorph(portrait, landscape), true);
});

test("only boxes near the viewport animate", () => {
  const box = { x: 0, y: 1000, width: 100, height: 100 };
  assert.equal(isNearViewport(box, 0, 600, 0), false);
  assert.equal(isNearViewport(box, 0, 600, 600), true);
  assert.equal(isNearViewport(box, 1050, 1650, 0), true);
  assert.equal(isNearViewport(box, 1100, 1700, 0), false);
});

test("anchor is the first card still visible", () => {
  const boxes = [0, 200, 400].map((y) => ({ x: 0, y, width: 100, height: 180 }));
  assert.equal(anchorIndex(boxes, -50), 0);
  assert.equal(anchorIndex(boxes, 190), 1);
  assert.equal(anchorIndex(boxes, 700), -1);
});
