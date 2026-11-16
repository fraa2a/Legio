import type { StarDensity } from "../../services/appearance";
import { channelsOf } from "../../services/color-tint";
import { seededRandom } from "../../utils/seeded-random";
import type { CanvasFrame, CanvasRenderer } from "./background-loop";

export interface Star {
  // Position as a share of the canvas, slightly past the edges so parallax never uncovers them.
  x: number;
  y: number;
  layer: 0 | 1 | 2;
  size: number;
  brightness: number;
  phase: number;
  speed: number;
}

export interface StarfieldConfig {
  density: StarDensity;
  color: string;
}

const maxStars = 900;
const layerShift = [4, 9, 16];
const layerSize = [0.7, 1, 1.4];
const layerBrightness = [0.45, 0.65, 0.9];
const densityScale: Record<StarDensity, number> = { low: 0.5, medium: 1, high: 1.7 };
const twinkleDepth = 0.18;

// The same seed always yields the same sky; a larger window only reveals more of the sequence.
export function starLayout(count: number, seed = 0x5eed): Star[] {
  const random = seededRandom(seed);
  return Array.from({ length: count }, (_, index) => {
    const layer = (index % 3) as 0 | 1 | 2;
    return {
      x: random() * 1.04 - 0.02,
      y: random() * 1.04 - 0.02,
      layer,
      size: layerSize[layer] * (0.7 + random() * 0.6),
      brightness: layerBrightness[layer] * (0.6 + random() * 0.4),
      phase: random() * Math.PI * 2,
      speed: 0.4 + random() * 0.9,
    };
  });
}

export function starCount(width: number, height: number, density: StarDensity, quality: number): number {
  const base = Math.min(500, Math.max(40, (width * height) / 5200));
  return Math.min(maxStars, Math.round(base * densityScale[density] * quality));
}

// Moves the parallax offset toward the pointer, by at most the deepest layer shift.
export function easeParallax(current: { x: number; y: number }, pointer: CanvasFrame["pointer"], width: number, height: number, delta: number): { x: number; y: number } {
  const target = pointer === null || width === 0 || height === 0
    ? { x: 0, y: 0 }
    : { x: Math.max(-1, Math.min(1, (pointer.x / width) * 2 - 1)), y: Math.max(-1, Math.min(1, (pointer.y / height) * 2 - 1)) };
  const follow = Math.min(1, delta / 400);
  return { x: current.x + (target.x - current.x) * follow, y: current.y + (target.y - current.y) * follow };
}

export function createStarfieldRenderer(canvas: HTMLCanvasElement): CanvasRenderer<StarfieldConfig> | null {
  const context = canvas.getContext("2d");
  if (context === null) return null;
  const stars = starLayout(maxStars);
  let config: StarfieldConfig = { density: "medium", color: "#ffffff" };
  let ink = "255, 255, 255";
  let width = 0;
  let height = 0;
  let offset = { x: 0, y: 0 };

  return {
    configure(next) {
      config = next;
      const [red, green, blue] = channelsOf(next.color);
      ink = `${red}, ${green}, ${blue}`;
    },
    resize(nextWidth, nextHeight, ratio) {
      width = nextWidth;
      height = nextHeight;
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
    },
    draw(frame) {
      offset = frame.reduced ? { x: 0, y: 0 } : easeParallax(offset, frame.pointer, width, height, frame.delta);
      context.clearRect(0, 0, width, height);
      const count = starCount(width, height, config.density, frame.quality);
      for (let layer = 0; layer < 3; layer += 1) {
        const shiftX = -offset.x * layerShift[layer];
        const shiftY = -offset.y * layerShift[layer];
        for (let index = layer; index < count; index += 3) {
          const star = stars[index];
          const twinkle = frame.reduced ? 1 : 1 - twinkleDepth * (0.5 + 0.5 * Math.sin(frame.time * star.speed + star.phase));
          context.fillStyle = `rgba(${ink}, ${(star.brightness * twinkle).toFixed(3)})`;
          context.fillRect(star.x * width + shiftX - star.size / 2, star.y * height + shiftY - star.size / 2, star.size, star.size);
        }
      }
    },
    animating: () => true,
    dispose() {
      context.clearRect(0, 0, width, height);
    },
  };
}
