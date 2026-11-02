import {
  anchorIndex,
  cubicBezier,
  isNearViewport,
  motionEasing,
  needsMorph,
  placementTransform,
  planMorph,
  type Box,
  type Placement,
} from "./morph-plan";

export const morphDuration = 340;
const steps = 12;
const ease = cubicBezier(...motionEasing);

interface CapturedCard {
  box: Box;
  image: HTMLImageElement | null;
}

export interface MorphCapture {
  cards: Map<HTMLElement, CapturedCard>;
  anchor: HTMLElement | null;
  scroller: HTMLElement;
  scrollTop: number;
  gridOffset: number;
}

export interface GridMorph {
  capture: () => MorphCapture;
  play: (capture: MorphCapture) => void;
  destroy: () => void;
}

function findScroller(element: HTMLElement): HTMLElement {
  for (let parent = element.parentElement; parent !== null; parent = parent.parentElement) {
    const overflow = getComputedStyle(parent).overflowY;
    if (overflow === "auto" || overflow === "scroll") return parent;
  }
  return document.documentElement;
}

function scrollerTop(scroller: HTMLElement): number {
  return scroller === document.documentElement ? 0 : scroller.getBoundingClientRect().top;
}

// Offsets are grid-relative (the grid is positioned) and ignore transforms, so an
// in-flight morph is read back from the computed transform of its top-left origin.
function visualBox(card: HTMLElement): Box {
  const box = { x: card.offsetLeft, y: card.offsetTop, width: card.offsetWidth, height: card.offsetHeight };
  if (card.getAnimations().length === 0) return box;
  const matrix = new DOMMatrixReadOnly(getComputedStyle(card).transform);
  return { x: box.x + matrix.m41, y: box.y + matrix.m42, width: box.width * matrix.a, height: box.height * matrix.d };
}

function layoutBox(card: HTMLElement): Box {
  return { x: card.offsetLeft, y: card.offsetTop, width: card.offsetWidth, height: card.offsetHeight };
}

function hasSize(box: Box): boolean {
  return box.width > 0 && box.height > 0;
}

function loadedImage(card: HTMLElement): HTMLImageElement | null {
  const image = card.firstElementChild?.querySelector("img");
  if (!(image instanceof HTMLImageElement) || !image.complete || image.naturalWidth === 0) return null;
  const clone = image.cloneNode() as HTMLImageElement;
  clone.alt = "";
  clone.draggable = false;
  return clone;
}

function createGhost(image: HTMLImageElement, box: Box): HTMLElement {
  const ghost = document.createElement("div");
  ghost.setAttribute("aria-hidden", "true");
  Object.assign(ghost.style, {
    position: "absolute",
    left: "0",
    top: "0",
    width: `${box.width}px`,
    height: `${box.height}px`,
    pointerEvents: "none",
  });
  Object.assign(image.style, { display: "block", width: "100%", height: "100%", objectFit: "cover" });
  ghost.append(image);
  return ghost;
}

function transformFrames(
  frames: { offset: number }[],
  placement: (index: number) => Placement,
  extra: (index: number) => Keyframe = () => ({}),
): Keyframe[] {
  return frames.map((frame, index) => ({
    offset: frame.offset,
    transform: placementTransform(placement(index)),
    transformOrigin: "0 0",
    ...extra(index),
  }));
}

/**
 * Morphs grid cards between two layouts (portrait/landscape covers). The caller
 * captures the old state before the DOM changes, then plays once it has updated.
 * Each card frame is scaled from its old box to its new one while its artwork and
 * content are counter-scaled, so nothing is stretched. The old artwork is cloned
 * and crossfaded out. Only cards near the viewport animate; the first visible
 * card keeps its place on screen by compensating the scroll position. A capture
 * taken mid-morph starts from the card's current on-screen box.
 */
export function createGridMorph(grid: HTMLElement): GridMorph {
  let animations: Animation[] = [];
  let ghosts: HTMLElement[] = [];

  function clear(): void {
    for (const animation of animations) animation.cancel();
    for (const ghost of ghosts) ghost.remove();
    animations = [];
    ghosts = [];
  }

  function capture(): MorphCapture {
    const scroller = findScroller(grid);
    const scrollTop = scroller.scrollTop;
    const gridOffset = grid.getBoundingClientRect().top - scrollerTop(scroller) + scrollTop;
    const cards = new Map<HTMLElement, CapturedCard>();
    for (const child of grid.children) {
      if (child instanceof HTMLElement) cards.set(child, { box: visualBox(child), image: null });
    }
    const entries = [...cards];
    const top = scrollTop - gridOffset;
    const bottom = top + scroller.clientHeight;
    const first = anchorIndex(entries.map(([, captured]) => captured.box), top);
    for (const [card, captured] of entries) {
      if (isNearViewport(captured.box, top, bottom, scroller.clientHeight)) captured.image = loadedImage(card);
    }
    return { cards, anchor: entries[first]?.[0] ?? null, scroller, scrollTop, gridOffset };
  }

  function play(captured: MorphCapture): void {
    clear();
    const { scroller, gridOffset } = captured;
    const anchor = captured.anchor !== null && grid.contains(captured.anchor) ? captured.anchor : null;
    const anchorBox = anchor === null ? undefined : captured.cards.get(anchor)?.box;
    if (anchor !== null && anchorBox !== undefined) {
      scroller.scrollTop = captured.scrollTop + anchor.offsetTop - anchorBox.y;
    }

    const top = scroller.scrollTop - gridOffset;
    const bottom = top + scroller.clientHeight;
    const margin = scroller.clientHeight;
    const radius = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;

    for (const [card, { box: from, image }] of captured.cards) {
      if (!card.isConnected || !grid.contains(card)) continue;
      const to = layoutBox(card);
      if (!hasSize(from) || !hasSize(to) || !needsMorph(from, to)) continue;
      if (!isNearViewport(from, top, bottom, margin) && !isNearViewport(to, top, bottom, margin)) continue;

      const frames = planMorph(from, to, radius, steps, ease);
      const options: KeyframeAnimationOptions = { duration: morphDuration, easing: "linear" };
      const [artwork, ...content] = Array.from(card.children).filter(
        (child): child is HTMLElement => child instanceof HTMLElement,
      );

      animations.push(
        card.animate(
          transformFrames(
            frames,
            (index) => frames[index].card,
            (index) => ({
              borderRadius: `${frames[index].radiusX}px / ${frames[index].radiusY}px`,
              clipPath: `inset(0 round ${frames[index].radiusX}px / ${frames[index].radiusY}px)`,
            }),
          ),
          options,
        ),
      );
      if (artwork !== undefined) {
        animations.push(artwork.animate(transformFrames(frames, (index) => frames[index].artwork), options));
      }
      for (const element of content) {
        animations.push(
          element.animate(
            transformFrames(frames, (index) => frames[index].content, (index) => ({
              opacity: frames[index].contentOpacity,
            })),
            options,
          ),
        );
      }
      if (image !== null) {
        const ghost = createGhost(image, from);
        card.insertBefore(ghost, content[0] ?? null);
        ghosts.push(ghost);
        const animation = ghost.animate(
          transformFrames(frames, (index) => frames[index].ghost, (index) => ({
            opacity: frames[index].ghostOpacity,
          })),
          { ...options, fill: "forwards" },
        );
        animation.onfinish = () => ghost.remove();
        animations.push(animation);
      }
    }
  }

  return { capture, play, destroy: clear };
}
