import { mixColors } from "../../services/ambient-palette";
import { channelsOf } from "../../services/color-tint";
import type { CanvasRenderer } from "./background-loop";

export interface AmbientConfig {
  background: string;
  colors: string[];
}

const fadeMs = 1400;
// Seconds per drift cycle of each blob; different periods keep the motion from repeating visibly.
const driftPeriods = [83, 107, 131];
const anchors = [[0.2, 0.25], [0.8, 0.35], [0.45, 0.85]];

// Blob centers as shares of the canvas, moving on slow ellipses around fixed anchors.
export function ambientCenters(time: number): [number, number][] {
  return anchors.map(([x, y], index) => {
    const angle = (time / driftPeriods[index]) * Math.PI * 2 + index * 2.1;
    return [x + Math.cos(angle) * 0.12, y + Math.sin(angle * 0.8) * 0.1];
  });
}

export function easeFade(progress: number): number {
  const t = Math.min(1, Math.max(0, progress));
  return t * t * (3 - 2 * t);
}

function rgba(color: string, alpha: number): string {
  const [red, green, blue] = channelsOf(color);
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

// A soft gradient of a few large radial blobs over the palette background. It is meant for a
// small canvas the compositor scales up, which also keeps the per-frame cost negligible.
export function createAmbientRenderer(canvas: HTMLCanvasElement): CanvasRenderer<AmbientConfig> | null {
  const context = canvas.getContext("2d", { alpha: false });
  if (context === null) return null;
  let background = "#000000";
  let from: string[] = [];
  let to: string[] = [];
  let progress = 1;
  let width = 0;
  let height = 0;

  return {
    configure(next) {
      const current = progress >= 1 ? to : mixColors(from, to, easeFade(progress));
      background = next.background;
      from = current.length === next.colors.length ? current : next.colors;
      to = next.colors;
      progress = from === to ? 1 : 0;
    },
    resize(nextWidth, nextHeight, ratio) {
      width = nextWidth;
      height = nextHeight;
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
    },
    draw(frame) {
      progress = frame.reduced ? 1 : Math.min(1, progress + frame.delta / fadeMs);
      const colors = progress >= 1 ? to : mixColors(from, to, easeFade(progress));
      context.fillStyle = background;
      context.fillRect(0, 0, width, height);
      const radius = Math.max(width, height) * 0.75;
      ambientCenters(frame.time).forEach(([x, y], index) => {
        const color = colors[index % Math.max(1, colors.length)];
        if (color === undefined) return;
        const gradient = context.createRadialGradient(x * width, y * height, 0, x * width, y * height, radius);
        gradient.addColorStop(0, rgba(color, 0.9));
        gradient.addColorStop(1, rgba(color, 0));
        context.fillStyle = gradient;
        context.fillRect(0, 0, width, height);
      });
    },
    animating: () => true,
    dispose() {
      from = [];
      to = [];
    },
  };
}
