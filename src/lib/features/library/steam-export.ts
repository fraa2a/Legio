import type { Game } from "../../services/local-state";
import type { SteamShortcutPreview } from "../../services/steam-shortcuts";

/** Legio can launch the game itself, which is what an exported shortcut asks for. */
export function isSteamExportable(game: Game): boolean {
  const steamGame = game.steamInstallPath !== null && game.steamAppId !== null;
  return steamGame || game.executablePath !== null;
}

export function previewCounts(preview: SteamShortcutPreview): { add: number; update: number; unchanged: number } {
  const counts = { add: 0, update: 0, unchanged: 0 };
  for (const item of preview.items) counts[item.action] += 1;
  return counts;
}

export function withSelection(ids: string[], id: string, selected: boolean): string[] {
  const rest = ids.filter((known) => known !== id);
  return selected ? [...rest, id] : rest;
}
