import type { GameScreenshot } from "../../services/game-screenshots";

export interface LocalImage {
  id: string;
  name: string;
  thumbUrl: string | null;
  fullUrl: string | null;
}

export function toLocalImages(entries: readonly GameScreenshot[], thumbs: Readonly<Record<string, string>>, fulls: Readonly<Record<string, string>>): LocalImage[] {
  return entries.map((entry) => ({ id: entry.id, name: entry.name, thumbUrl: thumbs[entry.id] ?? null, fullUrl: fulls[entry.id] ?? null }));
}

export function pageCountOf(total: number, pageSize: number): number {
  return pageSize > 0 ? Math.max(1, Math.ceil(total / pageSize)) : 1;
}

export function pageOffset(page: number, pageSize: number): number {
  return Math.max(0, page - 1) * pageSize;
}

// Runs `task` over `items` with at most `limit` in flight. A failed item stays blank; invoke() logs the error.
export async function runLimited<T>(items: readonly T[], limit: number, task: (item: T) => Promise<void>): Promise<void> {
  let next = 0;
  const worker = async (): Promise<void> => {
    while (next < items.length) {
      const item = items[next++] as T;
      await task(item).catch(() => undefined);
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
}
