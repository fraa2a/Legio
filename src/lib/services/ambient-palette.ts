import { get } from "svelte/store";
import { contrastRatio, fitOklch, oklchOf } from "./color-tint";
import { sampleImagePalette } from "./game-accent";
import { acquireSteamImage } from "./steam-details";
import { artworkDisplayWidth } from "../stores/artwork-display";

export interface AmbientGround {
  background: string;
  text: string;
  muted: string;
  accent: string;
}

export const ambientColorCount = 3;

// Lightness offsets from the palette background and chroma caps keep the gradient a tinted
// background, so text and muted text never lose contrast against it.
const lightnessRange = { dark: [0.03, 0.11], light: [0.02, 0.08] } as const;
const chromaCap = { dark: 0.075, light: 0.05 } as const;
const minimumTextContrast = 4.5;

// Sources for the gradient: the sampled hues, completed with neighbors of the first one, or
// the palette accent when the artwork is grey or missing.
function sourceColors(samples: string[], accent: string): string[] {
  const base = samples.length > 0 ? samples : [accent];
  const colors = base.slice(0, ambientColorCount);
  const { lightness, chroma, hue } = oklchOf(base[0]);
  const shifts = [32, -32];
  while (colors.length < ambientColorCount) colors.push(fitOklch(lightness, chroma, (hue + shifts[colors.length - 1] + 360) % 360));
  return colors;
}

function readable(color: string, ground: AmbientGround): boolean {
  return [ground.text, ground.muted].every((ink) =>
    contrastRatio(ink, color) >= Math.min(minimumTextContrast, contrastRatio(ink, ground.background)));
}

// Maps sampled artwork colors to background shades near the palette background, in OKLCH.
export function ambientColors(samples: string[], ground: AmbientGround, scheme: "dark" | "light"): string[] {
  const background = oklchOf(ground.background);
  const [near, far] = lightnessRange[scheme];
  const direction = scheme === "dark" ? 1 : -1;
  return sourceColors(samples, ground.accent).map((color) => {
    const source = oklchOf(color);
    const offset = near + (far - near) * (scheme === "dark" ? source.lightness : 1 - source.lightness);
    let chroma = Math.min(source.chroma, chromaCap[scheme]);
    let shift = offset;
    for (let attempt = 0; attempt < 24; attempt += 1) {
      const candidate = fitOklch(Math.min(1, Math.max(0, background.lightness + direction * shift)), chroma, source.hue);
      if (readable(candidate, ground)) return candidate;
      shift *= 0.85;
      chroma *= 0.85;
    }
    return ground.background;
  });
}

function labOf(hex: string): [number, number, number] {
  const { lightness, chroma, hue } = oklchOf(hex);
  return [lightness, chroma * Math.cos((hue * Math.PI) / 180), chroma * Math.sin((hue * Math.PI) / 180)];
}

// Interpolates two color lists of equal length through OKLab, so a cross-fade keeps its lightness.
export function mixColors(from: string[], to: string[], amount: number): string[] {
  const t = Math.min(1, Math.max(0, amount));
  return to.map((target, index) => {
    const source = from[index] ?? target;
    if (t === 1 || source === target) return target;
    const [l1, a1, b1] = labOf(source);
    const [l2, a2, b2] = labOf(target);
    const a = a1 + (a2 - a1) * t;
    const b = b1 + (b2 - b1) * t;
    return fitOklch(l1 + (l2 - l1) * t, Math.hypot(a, b), ((Math.atan2(b, a) * 180) / Math.PI + 360) % 360);
  });
}

// Samples the blurred hero the artwork pipeline already caches for the game.
export function loadHeroPalette(steamAppId: number): { promise: Promise<string[]>; cancel(): void } {
  const consumer = acquireSteamImage({
    steamAppId, asset: "hero_blur", fallbackAsset: null, index: null, full: false, displayWidth: get(artworkDisplayWidth),
  }, "decorative");
  let cancelled = false;
  const promise = consumer.promise.then(async (loaded) => {
    const image = new Image();
    image.src = loaded.url;
    await image.decode();
    return cancelled ? [] : sampleImagePalette(image);
  }).finally(() => consumer.release());
  return { promise, cancel() { cancelled = true; consumer.release(); } };
}
