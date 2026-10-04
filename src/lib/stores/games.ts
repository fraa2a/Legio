import { get } from "svelte/store";
import { getSteamAsset } from "../services/steam-details";
import {
  createGame,
  listGames,
  removeGame,
  updateGame,
  type CreateGameInput,
  type Game,
  type UpdateGameInput,
} from "../services/local-state";
import { createResource } from "./resource";

export const games = createResource<Game[]>([], listGames, (value) => value.length === 0);

const queuedHeroes: number[] = [];
const requestedHeroes = new Set<number>();
let activeHeroes = 0;
let cachingStarted = false;

function cacheNextHero(): void {
  while (activeHeroes < 2 && queuedHeroes.length > 0) {
    const appId = queuedHeroes.shift();
    if (appId === undefined) break;
    activeHeroes += 1;
    void getSteamAsset(appId, "hero").catch((error: unknown) => {
      requestedHeroes.delete(appId);
      console.warn(`Could not cache library hero for Steam App ID ${appId}`, error);
    }).finally(() => {
      activeHeroes -= 1;
      cacheNextHero();
    });
  }
}

export function startLibraryHeroCaching(): void {
  if (cachingStarted) return;
  cachingStarted = true;
  games.subscribe((state) => {
    for (const game of state.data) {
      if (game.steamAppId === null || requestedHeroes.has(game.steamAppId)) continue;
      requestedHeroes.add(game.steamAppId);
      queuedHeroes.push(game.steamAppId);
    }
    cacheNextHero();
  });
}

export function reconcileGame(game: Game): void {
  const current = get(games).data;
  const known = current.some((item) => item.id === game.id);
  games.set(known ? current.map((item) => (item.id === game.id ? game : item)) : [...current, game]);
}

export async function addGame(input: CreateGameInput): Promise<Game> {
  const game = await createGame(input);
  reconcileGame(game);
  return game;
}

export async function saveGame(input: UpdateGameInput): Promise<Game> {
  const game = await updateGame(input);
  reconcileGame(game);
  return game;
}

export async function deleteGame(id: string): Promise<void> {
  try {
    await removeGame(id);
  } finally {
    await games.load();
  }
}
