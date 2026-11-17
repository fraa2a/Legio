const bytesPerMegabyte = 1024 * 1024;

// Slider stops in MB/s. The last stop is unlimited, stored as 0 bytes.
const stops = [1, 2, 5, 10, 20, 50, 100, 200, 0];

export const unlimitedStop = stops.length - 1;

export function stopBytes(index: number): number {
  return stops[index] * bytesPerMegabyte;
}

// A saved limit between stops places the thumb on the nearest one.
export function nearestStop(bytes: number): number {
  if (bytes <= 0) return unlimitedStop;
  let nearest = 0;
  for (let index = 1; index < unlimitedStop; index += 1) {
    if (Math.abs(stopBytes(index) - bytes) < Math.abs(stopBytes(nearest) - bytes)) nearest = index;
  }
  return nearest;
}

export function formatMegabytes(bytes: number): string {
  return (bytes / bytesPerMegabyte).toFixed(2).replace(/\.?0+$/, "");
}
