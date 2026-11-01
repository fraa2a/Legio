export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Placement {
  x: number;
  y: number;
  scaleX: number;
  scaleY: number;
}

export interface MorphFrame {
  offset: number;
  card: Placement;
  radiusX: number;
  radiusY: number;
  artwork: Placement;
  ghost: Placement;
  content: Placement;
  contentOpacity: number;
  ghostOpacity: number;
}

export const motionEasing = [0.22, 1, 0.36, 1] as const;
export const motionEasingCss = `cubic-bezier(${motionEasing.join(", ")})`;

export function cubicBezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number {
  const axis = (a: number, b: number, s: number) => 3 * a * (1 - s) ** 2 * s + 3 * b * (1 - s) * s ** 2 + s ** 3;
  return (t) => {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    let low = 0;
    let high = 1;
    for (let i = 0; i < 24; i += 1) {
      const middle = (low + high) / 2;
      if (axis(x1, x2, middle) < t) low = middle;
      else high = middle;
    }
    return axis(y1, y2, (low + high) / 2);
  };
}

export function placementTransform(placement: Placement): string {
  return `translate(${placement.x}px, ${placement.y}px) scale(${placement.scaleX}, ${placement.scaleY})`;
}

export function needsMorph(from: Box, to: Box): boolean {
  return (
    Math.abs(from.x - to.x) > 0.5 ||
    Math.abs(from.y - to.y) > 0.5 ||
    Math.abs(from.width - to.width) > 0.5 ||
    Math.abs(from.height - to.height) > 0.5
  );
}

export function isNearViewport(box: Box, top: number, bottom: number, margin: number): boolean {
  return box.y + box.height > top - margin && box.y < bottom + margin;
}

// First box whose bottom edge is below the viewport top, or -1 when none is.
export function anchorIndex(boxes: readonly Box[], viewportTop: number): number {
  return boxes.findIndex((box) => box.y + box.height > viewportTop);
}

function smoothstep(from: number, to: number, value: number): number {
  const progress = Math.min(1, Math.max(0, (value - from) / (to - from)));
  return progress * progress * (3 - 2 * progress);
}

// A child of natural size `natural`, laid out inside a card that is scaled by
// (scaleX, scaleY), is counter-scaled so that it covers `frame` uniformly and
// stays centered on it, as object-cover would.
function coverPlacement(natural: Box, frame: Box, scaleX: number, scaleY: number): Placement {
  const cover = Math.max(frame.width / natural.width, frame.height / natural.height);
  return {
    x: (frame.width - natural.width * cover) / 2 / scaleX,
    y: (frame.height - natural.height * cover) / 2 / scaleY,
    scaleX: cover / scaleX,
    scaleY: cover / scaleY,
  };
}

/**
 * Plans a card morph from its old box to its new one as `steps + 1` keyframes
 * sampled along `ease`. The card frame is scaled, and everything inside it is
 * counter-scaled so that artwork and content are never stretched: the new
 * artwork and the old artwork (ghost) both cover the frame while the ghost
 * fades out, and the content fades in once the frame is close to its final size.
 * Boxes must have a positive size.
 */
export function planMorph(
  from: Box,
  to: Box,
  radius: number,
  steps: number,
  ease: (t: number) => number,
): MorphFrame[] {
  const natural: Box = { x: 0, y: 0, width: to.width, height: to.height };
  const old: Box = { x: 0, y: 0, width: from.width, height: from.height };
  const frames: MorphFrame[] = [];
  for (let i = 0; i <= steps; i += 1) {
    const offset = i / steps;
    const progress = ease(offset);
    const frame: Box = {
      x: from.x + (to.x - from.x) * progress,
      y: from.y + (to.y - from.y) * progress,
      width: from.width + (to.width - from.width) * progress,
      height: from.height + (to.height - from.height) * progress,
    };
    const scaleX = frame.width / to.width;
    const scaleY = frame.height / to.height;
    const size: Box = { x: 0, y: 0, width: frame.width, height: frame.height };
    frames.push({
      offset,
      card: { x: frame.x - to.x, y: frame.y - to.y, scaleX, scaleY },
      radiusX: radius / scaleX,
      radiusY: radius / scaleY,
      artwork: coverPlacement(natural, size, scaleX, scaleY),
      ghost: coverPlacement(old, size, scaleX, scaleY),
      content: { x: 0, y: 0, scaleX: 1 / scaleX, scaleY: 1 / scaleY },
      contentOpacity: smoothstep(0.2, 0.7, offset),
      ghostOpacity: 1 - progress,
    });
  }
  return frames;
}
