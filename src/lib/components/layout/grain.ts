import { seededRandom } from "../../utils/seeded-random";

export const grainTileSize = 128;
// Strongest per-pixel alpha at full intensity; the overlay opacity scales it down.
const maxAlpha = 40;

// White and black specks with an alpha that follows the noise, so the overlay adds texture
// and breaks gradient bands without shifting the average brightness.
export function grainPixels(size = grainTileSize, seed = 0x67a1): Uint8ClampedArray {
  const random = seededRandom(seed);
  const pixels = new Uint8ClampedArray(size * size * 4);
  for (let index = 0; index < pixels.length; index += 4) {
    const value = (random() + random()) - 1;
    const tone = value > 0 ? 255 : 0;
    pixels[index] = tone;
    pixels[index + 1] = tone;
    pixels[index + 2] = tone;
    pixels[index + 3] = Math.round(Math.abs(value) * maxAlpha);
  }
  return pixels;
}

let texture: string | null | undefined;

// Generated once per session and reused by every overlay as a tiled CSS background.
export function grainTexture(): string | null {
  if (texture !== undefined) return texture;
  try {
    const canvas = document.createElement("canvas");
    canvas.width = grainTileSize;
    canvas.height = grainTileSize;
    const context = canvas.getContext("2d");
    if (context === null) return (texture = null);
    const image = context.createImageData(grainTileSize, grainTileSize);
    image.data.set(grainPixels());
    context.putImageData(image, 0, 0);
    texture = canvas.toDataURL("image/png");
  } catch (error) {
    console.warn("Could not generate the grain texture", error);
    texture = null;
  }
  return texture;
}
