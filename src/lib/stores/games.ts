import { derived } from "svelte/store";
import { listGames, removeGame, type Game } from "../services/local-state";
import { createResource } from "./resource";

export const games = createResource<Game[]>([], listGames, (value) => value.length === 0);

export const gameCount = derived(games, (state) => state.data.length);

export const steamGameCount = derived(
  games,
  (state) => state.data.filter((game) => game.steamAppId !== null).length,
);

export async function deleteGame(id: string): Promise<void> {
  await removeGame(id);
  await games.load();
}
