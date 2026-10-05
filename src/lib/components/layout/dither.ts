const thresholds = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

export function drawDither(
  context: CanvasRenderingContext2D, width: number, height: number, color: string, time: number,
): void {
  const cell = 6;
  context.fillStyle = color;
  for (let y = 0; y < height; y += cell) {
    for (let x = 0; x < width; x += cell) {
      const wave = Math.sin(x / 280 + time) + Math.cos(y / 240 - time * 0.7)
        + Math.sin((x + y) / 360 + time * 0.4);
      const intensity = 0.12 + (wave + 3) / 6 * 0.48;
      const threshold = (thresholds[(y / cell % 4) * 4 + x / cell % 4] + 0.5) / 16;
      if (intensity > threshold) context.fillRect(x, y, cell - 1, cell - 1);
    }
  }
}
