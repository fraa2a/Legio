import { derived } from "svelte/store";
import { recentGames } from "../features/home/home-model";
import { activeSection, selectedGameId, selectedStoreGame } from "./navigation";
import { games } from "./games";
import { playtime } from "./playtime";

// The Steam app whose artwork colors the ambient background: the open game page, the last
// played game on the home, otherwise none.
export const ambientSteamAppId = derived(
  [activeSection, selectedGameId, selectedStoreGame, games, playtime],
  ([$section, $gameId, $storeGame, $games, $playtime]): number | null => {
    if ($section === "store") return $storeGame?.steamAppId ?? null;
    if ($section === "library") return $gameId === null ? null : $games.data.find((game) => game.id === $gameId)?.steamAppId ?? null;
    if ($section === "home") return recentGames($games.data, $playtime.data)[0]?.game.steamAppId ?? null;
    return null;
  },
);
