import { get } from "svelte/store";
import {
  createGame,
  listGames,
  listHiddenSteamGames,
  removeGame,
  uninstallGameFiles,
  setGameHidden,
  updateGame,
  type CreateGameInput,
  type Game,
  type UpdateGameInput,
} from "../services/local-state";
import { createResource } from "./resource";

export const games = createResource<Game[]>([], listGames, (value) => value.length === 0);
export const hiddenSteamGames = createResource<Game[]>([], listHiddenSteamGames);

export async function hideSteamGame(id: string): Promise<void> {
  await setGameHidden(id, true);
  await Promise.all([games.reload(), hiddenSteamGames.reload()]);
}

export async function restoreSteamGame(id: string): Promise<void> {
  await setGameHidden(id, false);
  await Promise.all([games.reload(), hiddenSteamGames.reload()]);
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
    await games.reload();
  }
}

export async function uninstallGame(id: string, installationRoot: string): Promise<{ warnings: string[] }> {
  try {
    return await uninstallGameFiles(id, installationRoot);
  } finally {
    await games.reload();
  }
}
