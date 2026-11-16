export const recapCardSize = { width: 1200, height: 630 } as const;

export interface RecapCardColors {
  background: string;
  surface: string;
  text: string;
  muted: string;
  accent: string;
}

export interface RecapCardGame {
  name: string;
  duration: string;
  share: number;
  shareLabel: string;
  // Only same-origin images, so the canvas stays exportable.
  image: CanvasImageSource | null;
}

export interface RecapCardInput {
  brand: string;
  heading: string;
  period: string;
  stats: { label: string; value: string }[];
  topTitle: string;
  games: RecapCardGame[];
  colors: RecapCardColors;
}

const font = (weight: number, size: number) => `${weight} ${size}px system-ui, "Segoe UI", sans-serif`;
const margin = 56;
const columnSplit = 600;
const rowHeight = 76;

export function fitText(context: Pick<CanvasRenderingContext2D, "measureText">, text: string, maxWidth: number): string {
  if (context.measureText(text).width <= maxWidth) return text;
  let end = text.length;
  while (end > 1 && context.measureText(`${text.slice(0, end)}...`).width > maxWidth) end--;
  return `${text.slice(0, end).trimEnd()}...`;
}

function roundedRect(context: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, radius: number): void {
  context.beginPath();
  context.roundRect(x, y, width, height, radius);
}

function drawCover(context: CanvasRenderingContext2D, game: RecapCardGame, x: number, y: number, size: number, colors: RecapCardColors): void {
  context.save();
  roundedRect(context, x, y, size, size, 10);
  context.clip();
  context.fillStyle = colors.surface;
  context.fillRect(x, y, size, size);
  if (game.image === null) {
    context.fillStyle = colors.muted;
    context.font = font(600, 28);
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(game.name.trim().charAt(0).toLocaleUpperCase() || "?", x + size / 2, y + size / 2);
  } else {
    context.drawImage(game.image, x, y, size, size);
  }
  context.restore();
}

export function drawRecapCard(context: CanvasRenderingContext2D, input: RecapCardInput): void {
  const { width, height } = recapCardSize;
  const { colors } = input;
  context.clearRect(0, 0, width, height);
  context.fillStyle = colors.background;
  context.fillRect(0, 0, width, height);
  context.fillStyle = colors.accent;
  context.fillRect(0, 0, 10, height);
  context.textAlign = "left";
  context.textBaseline = "alphabetic";

  context.fillStyle = colors.muted;
  context.font = font(500, 26);
  context.fillText(input.heading, margin, 84);
  context.textAlign = "right";
  context.fillStyle = colors.accent;
  context.font = font(700, 30);
  context.fillText(input.brand, width - margin, 84);
  context.textAlign = "left";
  context.fillStyle = colors.text;
  context.font = font(700, 84);
  context.fillText(fitText(context, input.period, columnSplit - margin * 2), margin, 184);

  const tileWidth = (columnSplit - margin - 24 - 16) / 2;
  input.stats.slice(0, 4).forEach((stat, index) => {
    const x = margin + (index % 2) * (tileWidth + 16);
    const y = 232 + Math.floor(index / 2) * 168;
    roundedRect(context, x, y, tileWidth, 152, 16);
    context.fillStyle = colors.surface;
    context.fill();
    context.fillStyle = colors.text;
    context.font = font(700, 40);
    context.fillText(fitText(context, stat.value, tileWidth - 40), x + 20, y + 78);
    context.fillStyle = colors.muted;
    context.font = font(500, 22);
    context.fillText(fitText(context, stat.label, tileWidth - 40), x + 20, y + 118);
  });

  const listX = columnSplit + 24;
  const listWidth = width - margin - listX;
  context.fillStyle = colors.text;
  context.font = font(600, 28);
  context.fillText(input.topTitle, listX, 128);
  input.games.slice(0, 5).forEach((game, index) => {
    const y = 164 + index * rowHeight;
    drawCover(context, game, listX, y, 56, colors);
    const textX = listX + 72;
    const textWidth = listWidth - 72;
    context.textAlign = "left";
    context.fillStyle = colors.text;
    context.font = font(600, 24);
    context.fillText(fitText(context, game.name, textWidth - 150), textX, y + 24);
    context.textAlign = "right";
    context.fillStyle = colors.muted;
    context.font = font(500, 22);
    context.fillText(`${game.duration} - ${game.shareLabel}`, listX + listWidth, y + 24);
    context.textAlign = "left";
    roundedRect(context, textX, y + 38, textWidth, 10, 5);
    context.fillStyle = colors.surface;
    context.fill();
    roundedRect(context, textX, y + 38, Math.max(10, textWidth * Math.min(1, Math.max(0, game.share))), 10, 5);
    context.fillStyle = colors.accent;
    context.fill();
  });
}

export function readRecapCardColors(element: Element): RecapCardColors {
  const style = getComputedStyle(element);
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    background: read("--legio-background", "#000000"),
    surface: read("--legio-surface", "#18181b"),
    text: read("--legio-text", "#fafafa"),
    muted: read("--legio-muted", "#a1a1aa"),
    accent: read("--legio-accent", "#fafafa"),
  };
}

// The card is drawn at its fixed size whatever the window looks like.
export async function renderRecapCardPng(input: RecapCardInput): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = recapCardSize.width;
  canvas.height = recapCardSize.height;
  const context = canvas.getContext("2d");
  if (context === null) throw new Error("Canvas is not available");
  drawRecapCard(context, input);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (blob === null) throw new Error("The image could not be created");
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000) binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  return btoa(binary);
}
