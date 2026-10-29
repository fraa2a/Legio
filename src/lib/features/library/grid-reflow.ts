import { reducedMotion } from "../../utils/motion";

const duration = 280;
const easing = "cubic-bezier(0.22, 1, 0.36, 1)";

interface Slot {
  left: number;
  top: number;
  row: number;
}

interface Glide {
  card: HTMLElement;
  x: number;
  y: number;
}

// offsetLeft/offsetTop ignore transforms, so an in-flight glide never skews the
// measurement. The grid must be positioned for the offsets to be grid-relative.
function measure(grid: HTMLElement): Map<HTMLElement, Slot> {
  const slots = new Map<HTMLElement, Slot>();
  let row = 0;
  let previousTop = 0;
  for (const child of grid.children) {
    if (!(child instanceof HTMLElement)) continue;
    if (slots.size > 0 && child.offsetTop - previousTop > 1) row += 1;
    previousTop = child.offsetTop;
    slots.set(child, { left: child.offsetLeft, top: child.offsetTop, row });
  }
  return slots;
}

function currentOffset(card: HTMLElement): { x: number; y: number } {
  const matrix = new DOMMatrixReadOnly(getComputedStyle(card).transform);
  return { x: matrix.m41, y: matrix.m42 };
}

/**
 * Glides cards that wrap to another row after the grid resizes (sidebar toggle,
 * window resize). Cards that stay on their row follow the grid directly, since
 * they are resized as well and a translation cannot express that.
 */
export function reflowOnResize(grid: HTMLElement): { destroy: () => void } {
  if (reducedMotion) return { destroy: () => undefined };

  const running = new Map<HTMLElement, Animation>();
  let slots = measure(grid);

  // ResizeObserver callbacks run once per frame between layout and paint, so the
  // new layout is never painted before the glide starts.
  const resizeObserver = new ResizeObserver(() => {
    const next = measure(grid);
    const glides: Glide[] = [];
    for (const [card, slot] of next) {
      const previous = slots.get(card);
      if (previous === undefined || previous.row === slot.row) continue;
      const inflight = running.has(card) ? currentOffset(card) : { x: 0, y: 0 };
      glides.push({
        card,
        x: previous.left - slot.left + inflight.x,
        y: previous.top - slot.top + inflight.y,
      });
    }
    slots = next;

    for (const { card, x, y } of glides) {
      running.get(card)?.cancel();
      const animation = card.animate(
        [{ transform: `translate(${x}px, ${y}px)` }, { transform: "translate(0, 0)" }],
        { duration, easing },
      );
      animation.onfinish = () => running.delete(card);
      running.set(card, animation);
    }
  });

  // Added or removed cards re-baseline the layout without gliding.
  const mutationObserver = new MutationObserver(() => {
    slots = measure(grid);
  });

  resizeObserver.observe(grid);
  mutationObserver.observe(grid, { childList: true });

  return {
    destroy: () => {
      resizeObserver.disconnect();
      mutationObserver.disconnect();
      for (const animation of running.values()) animation.cancel();
      running.clear();
    },
  };
}
