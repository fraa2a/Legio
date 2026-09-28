import { derived, get, writable } from "svelte/store";
import {
  cancelGameLaunch,
  inspectSteamGameLaunch,
  listGameLaunchStates,
  launchSteamGame,
  stopGame,
  type GameLaunchState,
} from "../services/steam-accounts";
import type { Game } from "../services/local-state";
import { toMessage } from "../utils/errors";
import { createResource } from "./resource";
import { appInfo } from "./app-info";
import { launchConfiguredGameWithRunner, launchNativeGame } from "../services/launch";
import { getCompatibilityLogsDirectory } from "../services/game-settings";
import { settings } from "./settings";
import { hideWindow } from "../services/window";

export const launchStates = createResource<GameLaunchState[]>([], listGameLaunchStates);

export const launchStateByGame = derived(launchStates, (state) => {
  const index = new Map<string, GameLaunchState>();
  for (const launch of state.data) index.set(launch.gameId, launch);
  return index;
});

export const hasPendingLaunch = derived(launchStates, (state) =>
  state.data.some((launch) => launch.status !== "idle"),
);

export const launchError = writable<string | null>(null);
export const pendingGameId = writable<string | null>(null);
export const cancelPendingGameId = writable<string | null>(null);
export const accountSwitchGame = writable<Game | null>(null);

let pollTimer: ReturnType<typeof setInterval> | null = null;
const hiddenForSession = new Set<string>();

launchStates.subscribe((state) => {
  for (const launch of state.data) {
    if (launch.status === "idle") hiddenForSession.delete(launch.gameId);
    else if (launch.status === "running" && !hiddenForSession.has(launch.gameId)) {
      hiddenForSession.add(launch.gameId);
      if (get(settings).data.hideOnGameStart) {
        void hideWindow().catch((error) => launchError.set(toMessage(error)));
      }
    }
  }
});

hasPendingLaunch.subscribe((pending) => {
  if (pending && pollTimer === null) {
    pollTimer = setInterval(() => void launchStates.load(), 2000);
  } else if (!pending && pollTimer !== null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
});

export async function playGame(game: Game): Promise<void> {
  launchError.set(null);
  pendingGameId.set(game.id);
  try {
    if (game.steamInstallPath !== null) {
      const inspection = await inspectSteamGameLaunch(game.id);
      if (inspection.status === "mismatch") {
        accountSwitchGame.set(game);
        return;
      }
      await launchSteamGame(game.id, false);
    } else if (game.executablePath !== null) {
      const platform = get(appInfo).data.platform;
      if (platform === "linux") {
        await launchConfiguredGameWithRunner(game.id);
      } else if (platform === "windows") {
        await launchNativeGame(game.id);
      } else {
        throw new Error("L'avvio di giochi manuali non è disponibile su questa piattaforma.");
      }
    } else {
      throw new Error("Seleziona un eseguibile nelle impostazioni del gioco.");
    }
    await launchStates.load();
  } catch (error) {
    let message = toMessage(error);
    if (game.steamInstallPath === null && game.executablePath !== null && get(appInfo).data.platform === "linux") {
      try {
        message += ` Log compatibilità: ${await getCompatibilityLogsDirectory()}`;
      } catch (diagnosticsError) {
        message += ` Impossibile trovare i log di compatibilità: ${toMessage(diagnosticsError)}`;
      }
    }
    launchError.set(message);
  } finally {
    pendingGameId.set(null);
  }
}

export function dismissAccountSwitch(): void {
  accountSwitchGame.set(null);
}

export async function confirmAccountSwitch(): Promise<void> {
  const game = get(accountSwitchGame);
  accountSwitchGame.set(null);
  if (game === null) return;
  launchError.set(null);
  pendingGameId.set(game.id);
  try {
    await launchSteamGame(game.id, true);
    await launchStates.load();
  } catch (error) {
    launchError.set(toMessage(error));
  } finally {
    pendingGameId.set(null);
  }
}

export async function abortGameLaunch(game: Game): Promise<void> {
  launchError.set(null);
  cancelPendingGameId.set(game.id);
  try {
    await cancelGameLaunch(game.id);
    await launchStates.load();
  } catch (error) {
    launchError.set(toMessage(error));
  } finally {
    cancelPendingGameId.set(null);
  }
}

export async function stopGameProcess(game: Game): Promise<void> {
  launchError.set(null);
  pendingGameId.set(game.id);
  try {
    await stopGame(game.id);
    await launchStates.load();
  } catch (error) {
    launchError.set(toMessage(error));
  } finally {
    pendingGameId.set(null);
  }
}
