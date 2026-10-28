export interface Tint {
  hue: number;
  chroma: number;
}

const toLinear = (value: number): number => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
const fromLinear = (value: number): number => (value <= 0.0031308 ? value * 12.92 : 1.055 * value ** (1 / 2.4) - 0.055);

function lightnessOf(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((index) => toLinear(Number.parseInt(hex.slice(index, index + 2), 16) / 255));
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
}

// Returns null when the color falls outside the sRGB gamut.
function oklchToHex(lightness: number, chroma: number, hue: number): string | null {
  const a = chroma * Math.cos((hue * Math.PI) / 180);
  const b = chroma * Math.sin((hue * Math.PI) / 180);
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const linear = [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
  if (linear.some((channel) => channel < -0.0005 || channel > 1.0005)) return null;
  return "#" + linear.map((channel) => Math.round(fromLinear(Math.min(1, Math.max(0, channel))) * 255).toString(16).padStart(2, "0")).join("");
}

// Keeps the lightness of a neutral and gives it the hue of the tint. Near-white
// shades fade the chroma out so highlights stay neutral.
export function tintColor(hex: string, tint: Tint): string {
  const lightness = lightnessOf(hex);
  let chroma = tint.chroma * (lightness > 0.92 ? Math.max(0, (1 - lightness) / 0.08) : 1);
  for (let attempt = 0; attempt < 16; attempt += 1) {
    const result = oklchToHex(lightness, chroma, tint.hue);
    if (result !== null) return result;
    chroma *= 0.8;
  }
  return hex;
}
