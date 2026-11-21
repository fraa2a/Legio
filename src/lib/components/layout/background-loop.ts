export interface CanvasFrame {
  // Seconds of animation, frozen under reduced motion.
  time: number;
  // Milliseconds since the previous drawn frame, 0 for the first one.
  delta: number;
  // Share of the full detail to draw, lowered when frames run long.
  quality: number;
  reduced: boolean;
  // Pointer in CSS pixels relative to the canvas, null when unknown or under reduced motion.
  pointer: { x: number; y: number } | null;
}

export interface CanvasRenderer<C> {
  configure(config: C): void;
  // CSS size of the canvas and the canvas pixels per CSS pixel.
  resize(width: number, height: number, ratio: number): void;
  draw(frame: CanvasFrame): void;
  // Whether the next frame differs from the last one without new input.
  animating(): boolean;
  dispose(): void;
}

export interface QualityState {
  level: number;
  slow: number;
}

export const qualityLevels = [1, 0.75, 0.5, 0.35] as const;
const slowFramesToStepDown = 30;
// Longer gaps are stalls (a hidden window, a blocked thread), not rendering cost.
const stallMs = 250;

export function initialQuality(): QualityState {
  return { level: 0, slow: 0 };
}

// Lowers the detail one step after a run of long frames. It never raises it again while
// mounted, so a busy system does not make the background oscillate.
export function stepQuality(state: QualityState, frameMs: number, targetMs: number): QualityState {
  if (frameMs <= 0 || frameMs > stallMs) return state;
  const slow = frameMs > targetMs * 1.35 ? state.slow + 1 : Math.max(0, state.slow - 1);
  if (slow < slowFramesToStepDown) return { level: state.level, slow };
  return { level: Math.min(qualityLevels.length - 1, state.level + 1), slow: 0 };
}

export interface BackgroundLoopOptions<C> {
  container: HTMLElement;
  canvas: HTMLCanvasElement;
  renderer: CanvasRenderer<C>;
  config: C;
  active: boolean;
  // Canvas pixels per CSS pixel before the device ratio, below 1 for soft content the compositor scales up.
  resolution?: number;
  maxFps?: number;
  pointer?: boolean;
}

export interface BackgroundLoop<C> {
  setActive(active: boolean): void;
  configure(config: C): void;
  dispose(): void;
}

// Drives a canvas renderer: sizing with a capped device ratio, one frame per change while
// paused for reduced motion or inactive, a frame-time guard and context recovery.
export function startBackgroundLoop<C>(options: BackgroundLoopOptions<C>): BackgroundLoop<C> {
  const { container, canvas, renderer } = options;
  const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
  const interval = 1000 / Math.min(60, options.maxFps ?? 60);
  let active = options.active;
  let configKey = JSON.stringify(options.config);
  let width = 0;
  let height = 0;
  let ratio = 1;
  let frame = 0;
  let time = 0;
  let previous: number | null = null;
  let stale = true;
  let lost = false;
  let quality = initialQuality();
  let pointer: { x: number; y: number } | null = null;

  // An inactive background still draws its pending change once, so it never stays blank.
  function schedule(): void {
    if ((active || stale) && !lost && frame === 0) frame = requestAnimationFrame(render);
  }

  function cancel(): void {
    cancelAnimationFrame(frame);
    frame = 0;
    previous = null;
  }

  function render(now: number): void {
    frame = 0;
    if ((!active && !stale) || lost || width === 0 || height === 0) return;
    const reduced = motion.matches;
    if (!stale && previous !== null && now - previous < interval - 2) {
      schedule();
      return;
    }
    const delta = previous === null ? 0 : Math.min(now - previous, 100);
    if (!reduced && previous !== null) {
      quality = stepQuality(quality, now - previous, interval);
      time += delta / 1000;
    }
    renderer.draw({ time, delta, quality: qualityLevels[quality.level], reduced, pointer: reduced ? null : pointer });
    stale = false;
    if (active && !reduced && renderer.animating()) {
      previous = now;
      schedule();
    } else previous = null;
  }

  function invalidate(): void {
    stale = true;
    schedule();
  }

  function resize(): void {
    const bounds = container.getBoundingClientRect();
    const nextRatio = Math.min(window.devicePixelRatio || 1, 2) * (options.resolution ?? 1);
    if (bounds.width === width && bounds.height === height && nextRatio === ratio) return;
    width = bounds.width;
    height = bounds.height;
    ratio = nextRatio;
    canvas.width = Math.max(1, Math.round(width * ratio));
    canvas.height = Math.max(1, Math.round(height * ratio));
    renderer.resize(width, height, ratio);
    invalidate();
  }

  function moved(event: PointerEvent): void {
    if (!active || motion.matches) return;
    const bounds = container.getBoundingClientRect();
    pointer = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
    schedule();
  }

  function motionChanged(): void {
    cancel();
    invalidate();
  }

  function contextLost(event: Event): void {
    event.preventDefault();
    lost = true;
    cancel();
  }

  function contextRestored(): void {
    lost = false;
    renderer.resize(width, height, ratio);
    invalidate();
  }

  renderer.configure(options.config);
  const observer = new ResizeObserver(resize);
  observer.observe(container);
  if (options.pointer) window.addEventListener("pointermove", moved);
  motion.addEventListener("change", motionChanged);
  canvas.addEventListener("contextlost", contextLost);
  canvas.addEventListener("contextrestored", contextRestored);
  resize();

  return {
    setActive(enabled) {
      if (enabled === active) return;
      active = enabled;
      cancel();
      if (active) invalidate();
    },
    configure(config) {
      const key = JSON.stringify(config);
      if (key === configKey) return;
      configKey = key;
      renderer.configure(config);
      invalidate();
    },
    dispose() {
      active = false;
      cancel();
      observer.disconnect();
      if (options.pointer) window.removeEventListener("pointermove", moved);
      motion.removeEventListener("change", motionChanged);
      canvas.removeEventListener("contextlost", contextLost);
      canvas.removeEventListener("contextrestored", contextRestored);
      renderer.dispose();
      // Drops the backing store at once instead of waiting for garbage collection.
      canvas.width = 0;
      canvas.height = 0;
    },
  };
}
