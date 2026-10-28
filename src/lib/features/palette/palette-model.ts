export type PaletteKind = "section" | "game" | "setting" | "action";

export interface PaletteItem {
  id: string;
  kind: PaletteKind;
  label: string;
  hint?: string;
  keywords?: string;
}

const fold = (value: string): string => value.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().trim();

// Lower is better; null means the query does not match.
function score(item: PaletteItem, query: string): number | null {
  const label = fold(item.label);
  if (label.startsWith(query)) return 0;
  if (label.split(/[\s\-_:.]+/).some((word) => word.startsWith(query))) return 1;
  if (label.includes(query)) return 2;
  if (item.keywords !== undefined && fold(item.keywords).includes(query)) return 3;
  let position = 0;
  for (const character of label) if (character === query[position]) position += 1;
  return position === query.length ? 4 : null;
}

// Items keep their given order inside a score, so callers list the likeliest targets first.
export function rankPaletteItems<T extends PaletteItem>(items: readonly T[], query: string, limit = 12): T[] {
  const needle = fold(query);
  if (needle === "") return items.slice(0, limit);
  return items
    .map((item, index) => ({ item, index, rank: score(item, needle) }))
    .filter((entry): entry is { item: T; index: number; rank: number } => entry.rank !== null)
    .sort((a, b) => a.rank - b.rank || a.index - b.index)
    .slice(0, limit)
    .map((entry) => entry.item);
}
