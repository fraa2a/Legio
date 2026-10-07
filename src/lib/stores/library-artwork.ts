import { derived, get } from "svelte/store";
import { recentGames } from "../features/home/home-model";
import { prefetchSteamHero } from "../services/steam-details";
import { games } from "./games";
import { playtime } from "./playtime";
import { hasPendingLaunch } from "./launch";
import { windowActive } from "./window-activity";

let started = false;
let active = 0;
const requested = new Set<number>();
const candidates = derived([games, playtime], ([$games, $playtime]) =>
  recentGames($games.data, $playtime.data).slice(0, 6).flatMap(({ game }) => game.steamAppId === null ? [] : [game.steamAppId]),
);
const enabled = derived([windowActive, hasPendingLaunch], ([$active, $pending]) => $active && !$pending);

function prefetch(): void {
  if (!get(enabled)) return;
  for (const appId of get(candidates)) {
    if (active >= 2) break;
    if (requested.has(appId)) continue;
    requested.add(appId);
    active++;
    void prefetchSteamHero(appId).catch((error: unknown) => {
      console.warn(`Could not cache recent hero for Steam App ID ${appId}`, error);
    }).finally(() => { active--; prefetch(); });
  }
}

export function startLibraryHeroCaching(): void {
  if (started) return;
  started = true;
  candidates.subscribe(prefetch);
  enabled.subscribe(prefetch);
}
