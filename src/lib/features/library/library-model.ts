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
