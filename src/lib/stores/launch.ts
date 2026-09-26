import { derived } from "svelte/store";
import {
  cancelGameLaunch,
  listGameLaunchStates,
  launchSteamGame,
  stopGame,
  type GameLaunchState,
} from "../services/steam-accounts";
import { createResource } from "./resource";

export const launchStates = createResource<GameLaunchState[]>([], listGameLaunchStates);

export const launchStateByGame = derived(launchStates, (state) => {
  const index = new Map<string, GameLaunchState>();
  for (const launch of state.data) index.set(launch.gameId, launch);
  return index;
});

export const hasPendingLaunch = derived(launchStates, (state) =>
  state.data.some((launch) => launch.status !== "idle"),
);

export async function requestLaunch(gameId: string, confirmAccountSwitch: boolean): Promise<void> {
  await launchSteamGame(gameId, confirmAccountSwitch);
  await launchStates.load();
}

export async function abortLaunch(gameId: string): Promise<void> {
  await cancelGameLaunch(gameId);
  await launchStates.load();
}

export async function stopRunningGame(gameId: string): Promise<void> {
  await stopGame(gameId);
  await launchStates.load();
}
