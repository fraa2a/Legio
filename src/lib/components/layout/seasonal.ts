import type { Season } from "../../services/appearance";
import { channelsOf, fitOklch } from "../../services/color-tint";
import { seededRandom } from "../../utils/seeded-random";
import type { CanvasRenderer } from "./background-loop";
import { seedParticles, stepParticle, type Particle } from "./particle-engine";

export interface SeasonalConfig {
  season: Season;
  color: string;
  scheme: "dark" | "light";
}

interface Flake extends Particle {
  size: number;
  phase: number;
}

interface SeasonShape {
  // Particles per million CSS pixels, and the bounds at any window size.
  density: number;
  min: number;
  max: number;
  velocity: (random: () => number) => [number, number];
  size: [number, number];
  // Horizontal sway in CSS pixels per frame.
  sway: number;
}

export const seasonShapes: Record<Season, SeasonShape> = {
  snow: { density: 55, min: 30, max: 140, velocity: (random) => [(random() - 0.5) * 0.3, 0.35 + random() * 0.65], size: [1, 2.6], sway: 0.25 },
  rain: { density: 70, min: 40, max: 160, velocity: (random) => [1.3, 7 + random() * 4], size: [10, 18], sway: 0 },
  leaves: { density: 12, min: 8, max: 34, velocity: (random) => [0.2 + random() * 0.3, 0.5 + random() * 0.6], size: [4, 7], sway: 0.6 },
};

const leafHues = [28, 48, 62];
const margin = 24;

export function seasonCount(season: Season, width: number, height: number, quality: number): number {
  const shape = seasonShapes[season];
  return Math.round(Math.min(shape.max, Math.max(shape.min, (width * height * shape.density) / 1e6)) * quality);
}

export function seedSeason(season: Season, count: number, width: number, height: number, seed = 0x5ea5): Flake[] {
  const random = seededRandom(seed);
  const shape = seasonShapes[season];
  return seedParticles(count, width, height, random, shape.velocity).map((particle) => ({
    ...particle,
    size: shape.size[0] + random() * (shape.size[1] - shape.size[0]),
    phase: random() * Math.PI * 2,
  }));
}

// Advances the particles by `delta` milliseconds and returns them, falling and wrapping.
export function stepSeason(flakes: Flake[], season: Season, width: number, height: number, time: number, delta: number): Flake[] {
  const scale = delta / (1000 / 60);
  const sway = seasonShapes[season].sway;
  for (const flake of flakes) {
    flake.x += Math.sin(time * 0.8 + flake.phase) * sway * scale;
    stepParticle(flake, width, height, "wrap", scale, margin);
  }
  return flakes;
}

function rgba(color: string, alpha: number): string {
  const [red, green, blue] = channelsOf(color);
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

export function createSeasonalRenderer(canvas: HTMLCanvasElement): CanvasRenderer<SeasonalConfig> | null {
  const context = canvas.getContext("2d");
  if (context === null) return null;
  let config: SeasonalConfig = { season: "snow", color: "#ffffff", scheme: "dark" };
  let flakes: Flake[] = [];
  let fill = "";
  let leafColors: string[] = [];
  let width = 0;
  let height = 0;
  let quality = 1;

  function reseed(): void {
    flakes = seedSeason(config.season, seasonCount(config.season, width, height, quality), width, height);
  }

  return {
    configure(next) {
      const reshape = next.season !== config.season;
      config = next;
      fill = rgba(next.color, next.season === "rain" ? 0.32 : 0.7);
      leafColors = leafHues.map((hue) => rgba(fitOklch(next.scheme === "dark" ? 0.66 : 0.58, 0.12, hue), 0.75));
      if (reshape) reseed();
    },
    resize(nextWidth, nextHeight, ratio) {
      width = nextWidth;
      height = nextHeight;
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      reseed();
    },
    draw(frame) {
      if (frame.quality !== quality) {
        quality = frame.quality;
        flakes = flakes.slice(0, seasonCount(config.season, width, height, quality));
      }
      if (!frame.reduced) stepSeason(flakes, config.season, width, height, frame.time, frame.delta);
      context.clearRect(0, 0, width, height);
      if (config.season === "rain") {
        context.strokeStyle = fill;
        context.lineWidth = 1;
        context.beginPath();
        for (const drop of flakes) {
          const length = drop.size / Math.hypot(drop.vx, drop.vy);
          context.moveTo(drop.x, drop.y);
          context.lineTo(drop.x - drop.vx * length, drop.y - drop.vy * length);
        }
        context.stroke();
      } else if (config.season === "snow") {
        context.fillStyle = fill;
        context.beginPath();
        for (const flake of flakes) {
          context.moveTo(flake.x + flake.size, flake.y);
          context.arc(flake.x, flake.y, flake.size, 0, Math.PI * 2);
        }
        context.fill();
      } else {
        leafColors.forEach((color, group) => {
          context.fillStyle = color;
          context.beginPath();
          for (let index = group; index < flakes.length; index += leafColors.length) {
            const leaf = flakes[index];
            const angle = frame.time * 0.6 + leaf.phase;
            context.moveTo(leaf.x + Math.cos(angle) * leaf.size, leaf.y + Math.sin(angle) * leaf.size);
            context.ellipse(leaf.x, leaf.y, leaf.size, leaf.size * 0.45, angle, 0, Math.PI * 2);
          }
          context.fill();
        });
      }
    },
    animating: () => true,
    dispose() {
      flakes = [];
      context.clearRect(0, 0, width, height);
    },
  };
}
