import { contrastRatio, fitOklch, oklchOf } from "./color-tint";

const sampleSize = 32;
const hueBins = 24;
const minSampleChroma = 0.04;
// Above the 4.5:1 text threshold so the 8-bit rounding of the final color cannot dip below it.
const accentContrast = 4.6;

// Groups the vivid pixels of an RGBA buffer by hue and returns the hue families from most to least
// vivid, at most `limit` of them. Families weaker than a tenth of the first one are dropped.
export function dominantPalette(pixels: ArrayLike<number>, limit = 4): string[] {
  const weight = new Array<number>(hueBins).fill(0);
  const chromaSum = new Array<number>(hueBins).fill(0);
  const lightnessSum = new Array<number>(hueBins).fill(0);
  const cosSum = new Array<number>(hueBins).fill(0);
  const sinSum = new Array<number>(hueBins).fill(0);
  for (let index = 0; index + 3 < pixels.length; index += 4) {
    if (pixels[index + 3] < 128) continue;
    const hex = "#" + [0, 1, 2].map((offset) => pixels[index + offset].toString(16).padStart(2, "0")).join("");
    const { lightness, chroma, hue } = oklchOf(hex);
    if (chroma < minSampleChroma || lightness < 0.15 || lightness > 0.97) continue;
    const bin = Math.floor(hue / (360 / hueBins)) % hueBins;
    const share = chroma * chroma;
    weight[bin] += share;
    chromaSum[bin] += share * chroma;
    lightnessSum[bin] += share * lightness;
    cosSum[bin] += share * Math.cos((hue * Math.PI) / 180);
    sinSum[bin] += share * Math.sin((hue * Math.PI) / 180);
  }
  const neighbors = (bin: number): number[] => [(bin + hueBins - 1) % hueBins, (bin + 1) % hueBins];
  // A hue spread over two neighboring bins still counts as one.
  const score = weight.map((value, bin) => value + 0.5 * neighbors(bin).reduce((sum, other) => sum + weight[other], 0));
  const used = new Array<boolean>(hueBins).fill(false);
  const colors: string[] = [];
  let first = 0;
  while (colors.length < limit) {
    let best = -1;
    for (let bin = 0; bin < hueBins; bin += 1) if (!used[bin] && weight[bin] > 0 && (best < 0 || score[bin] > score[best])) best = bin;
    if (best < 0 || (colors.length > 0 && weight[best] < first / 10)) break;
    if (colors.length === 0) first = weight[best];
    used[best] = true;
    for (const other of neighbors(best)) used[other] = true;
    const hue = ((Math.atan2(sinSum[best], cosSum[best]) * 180) / Math.PI + 360) % 360;
    colors.push(fitOklch(lightnessSum[best] / weight[best], chromaSum[best] / weight[best], hue));
  }
  return colors;
}

// Picks the most vivid hue of an RGBA pixel buffer. Returns null for grey or unreadable artwork.
export function dominantColor(pixels: ArrayLike<number>): string | null {
  return dominantPalette(pixels, 1)[0] ?? null;
}

export interface AccentGround {
  background: string;
  surface: string;
  text: string;
}

// Moves a color along OKLCH lightness until it reads as text on the background and the surface,
// which also covers focus rings. Falls back to the palette text color.
export function adjustAccent(color: string, ground: AccentGround, scheme: "dark" | "light"): string {
  const { lightness, chroma, hue } = oklchOf(color);
  const dark = scheme === "dark";
  const capped = Math.min(chroma, 0.16);
  const step = dark ? 0.01 : -0.01;
  let target = dark ? Math.min(0.92, Math.max(0.68, lightness)) : Math.min(0.55, Math.max(0.32, lightness));
  for (let attempt = 0; attempt < 100 && target >= 0 && target <= 1; attempt += 1) {
    const candidate = fitOklch(target, capped, hue);
    if (Math.min(contrastRatio(candidate, ground.background), contrastRatio(candidate, ground.surface)) >= accentContrast) return candidate;
    target += step;
  }
  return ground.text;
}

export function sampleImagePalette(image: HTMLImageElement): string[] {
  try {
    const canvas = document.createElement("canvas");
    canvas.width = sampleSize;
    canvas.height = sampleSize;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (context === null) return [];
    context.drawImage(image, 0, 0, sampleSize, sampleSize);
    return dominantPalette(context.getImageData(0, 0, sampleSize, sampleSize).data);
  } catch {
    return [];
  }
}

export function sampleImageColor(image: HTMLImageElement): string | null {
  return sampleImagePalette(image)[0] ?? null;
}
