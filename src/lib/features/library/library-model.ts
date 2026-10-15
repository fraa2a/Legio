import type { Game } from "../../services/local-state";
import type { PlaytimeSummary } from "../../services/playtime";
import type { GameLaunchState } from "../../services/steam-accounts";

const collator = new Intl.Collator(undefined, { sensitivity: "base" });

export function libraryGroups(games: Game[], query: string): Game[][] {
  const needle = query.trim().toLowerCase();
  const groups = new Map<string, Game[]>();
  for (const game of games) {
    const key = game.steamAppId === null ? game.id : `steam:${game.steamAppId}`;
    const group = groups.get(key);
    if (group === undefined) groups.set(key, [game]);
    else group.push(game);
  }
  return [...groups.values()].filter((group) => !needle || group.some((game) => game.name.toLowerCase().includes(needle)));
}

export function libraryItems(groups: Game[][], query: string, summaries: Map<string, PlaytimeSummary>, launches: Map<string, GameLaunchState>) {
  const needle = query.trim().toLowerCase();
  return groups.map((group) => ({
    game: group.find((game) => launches.get(game.id)?.status === "running" && (!needle || game.name.toLowerCase().includes(needle)))
      ?? (needle ? group.find((game) => game.name.toLowerCase().includes(needle)) : undefined)
      ?? group.find((game) => game.steamInstallPath !== null)
      ?? group[0],
    totalMilliseconds: group.reduce((total, game) => total + (summaries.get(game.id)?.totalMilliseconds ?? 0), 0),
  })).sort((left, right) => collator.compare(left.game.name, right.game.name));
}

export function nextVersionAfterRemoval(removed: Game, remaining: Game[], summaries: PlaytimeSummary[]): string | null {
  if (removed.steamAppId === null) return null;
  const lastPlayed = new Map(summaries.map((summary) => [summary.gameId, summary.lastPlayedAt ?? -1]));
  let selected: Game | null = null;
  for (const candidate of remaining) {
    if (candidate.id === removed.id || candidate.steamAppId !== removed.steamAppId) continue;
    if (selected === null || (lastPlayed.get(candidate.id) ?? -1) > (lastPlayed.get(selected.id) ?? -1)) selected = candidate;
  }
  return selected?.id ?? null;
}
