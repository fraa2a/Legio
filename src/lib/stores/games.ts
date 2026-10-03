import { get } from "svelte/store";
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
