import { get } from "svelte/store";
import {
  listSavedSteamAccounts,
  setGameSteamAccountPreference,
  type SavedSteamAccounts,
} from "../services/steam-accounts";
import { reconcileGame } from "./games";
import { createResource } from "./resource";

const empty: SavedSteamAccounts = { accounts: [], diagnostics: [] };

export const savedSteamAccounts = createResource<SavedSteamAccounts>(
  empty,
  listSavedSteamAccounts,
  (value) => value.accounts.length === 0,
);

export function ensureSavedSteamAccounts(): void {
  const { status } = get(savedSteamAccounts);
  // A failed attempt must not pin the selector to an error for the session.
  if (status !== "idle" && status !== "error") return;
  void savedSteamAccounts.load();
}

export async function saveGameSteamAccount(gameId: string, steamId: string | null): Promise<void> {
  reconcileGame(await setGameSteamAccountPreference(gameId, steamId));
}
