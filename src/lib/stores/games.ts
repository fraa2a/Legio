import { derived, get } from "svelte/store";
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

export const gameCount = derived(games, (state) => state.data.length);

export const steamGameCount = derived(
  games,
  (state) => state.data.filter((game) => game.steamAppId !== null).length,
);

export const manualGameCount = derived(
  games,
  (state) => state.data.filter((game) => game.steamAppId === null).length,
);

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
  await removeGame(id);
  await games.load();
}
