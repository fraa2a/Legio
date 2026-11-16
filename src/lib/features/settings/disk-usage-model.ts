import type { GameLibrary } from "../../services/game-libraries";
import type { GameSize, LibraryUsage } from "../../services/disk-usage";
import type { Game } from "../../services/local-state";
import type { PlaytimeSummary } from "../../services/playtime";

export interface SizeRow {
  game: Game;
  /** Null while the size has not been measured. */
  bytes: number | null;
  partial: boolean;
  /** Root of the Legio library that holds the game, null for Steam and external games. */
  library: string | null;
  lastPlayedAt: number | null;
  /** Same rule as the uninstall panel on the game page. */
  removable: boolean;
}

export interface UsageSegments {
  legio: number;
  other: number;
  free: number;
}

function compareNames(left: SizeRow, right: SizeRow): number {
  return left.game.name.localeCompare(right.game.name);
}

/** One row per game with a known folder, largest first, unmeasured games last by name. */
export function buildSizeRows(games: readonly Game[], sizes: readonly GameSize[], libraries: readonly GameLibrary[], summaries: readonly PlaytimeSummary[]): SizeRow[] {
  const sizeById = new Map(sizes.map((size) => [size.gameId, size]));
  const lastPlayed = new Map(summaries.map((summary) => [summary.gameId, summary.lastPlayedAt]));
  const libraryById = new Map(libraries.flatMap((library) => library.games.map((game) => [game.id, library.root] as const)));
  return games
    .filter((game) => game.steamInstallPath !== null || game.installationRoot !== null)
    .map((game): SizeRow => {
      const size = sizeById.get(game.id);
      return {
        game,
        bytes: size?.bytes ?? null,
        partial: size?.partial ?? false,
        library: libraryById.get(game.id) ?? null,
        lastPlayedAt: lastPlayed.get(game.id) ?? null,
        removable: game.steamInstallPath === null && game.executablePath !== null,
      };
    })
    .sort((left, right) => {
      if (left.bytes === null || right.bytes === null) return left.bytes === right.bytes ? compareNames(left, right) : left.bytes === null ? 1 : -1;
      return right.bytes - left.bytes || compareNames(left, right);
    });
}

export function totalBytes(rows: readonly SizeRow[]): number {
  return rows.reduce((sum, row) => sum + (row.bytes ?? 0), 0);
}

/** Percentages of the disk for the usage bar, or null when the disk size is unknown. */
export function usageSegments(usage: LibraryUsage): UsageSegments | null {
  const total = usage.totalBytes;
  if (total === null || total <= 0 || usage.freeBytes === null) return null;
  const share = (bytes: number) => Math.min(100, Math.max(0, (bytes / total) * 100));
  const legio = share(usage.legioBytes);
  const other = Math.min(100 - legio, share(usage.otherBytes ?? 0));
  return { legio, other, free: Math.max(0, 100 - legio - other) };
}
