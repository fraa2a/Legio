import { showToast } from "./toast";
import { windowActive } from "./window-activity";
import { t } from "../i18n";
import { derived, get, writable } from "svelte/store";
import {
  cancelGameLaunch,
  inspectSteamGameLaunch,
  listGameLaunchStates,
  onGameLaunchStates,
  onShortcutLaunchFailure,
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
import { hideWindow, showWindow } from "../services/window";

export const launchStates = createResource<GameLaunchState[]>([], listGameLaunchStates);

export const launchStateByGame = derived(launchStates, (state) => {
  const index = new Map<string, GameLaunchState>();
  for (const launch of state.data) index.set(launch.gameId, launch);
  return index;
});

export const hasPendingLaunch = derived(launchStates, (state) =>
  state.data.some((launch) => launch.status !== "idle"),
);

export const hasRunningGame = derived(launchStates, (state) =>
  state.data.some((launch) => launch.status === "running"),
);

export const launchError = writable<string | null>(null);
export const pendingGameId = writable<string | null>(null);
export const cancelPendingGameId = writable<string | null>(null);
export const accountSwitchGame = writable<Game | null>(null);

let eventsReady = false;
let listening: Promise<void> | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;
const hiddenForSession = new Set<string>();
const restoreOnExit = new Set<string>();
const reportedErrors = new Map<string, string>();
const reportedLogErrors = new Map<string, string>();
launchError.subscribe((message) => { if (message) showToast(message, "error"); });

launchStates.subscribe((state) => {
  for (const launch of state.data) {
    if (launch.error && reportedErrors.get(launch.gameId) !== launch.error) {
      reportedErrors.set(launch.gameId, launch.error);
      const logPath = launch.compatibilityLogPath;
      showToast(logPath ? `${launch.error} ${t("Log degli avvii: {0}", undefined, [logPath])}` : launch.error, "error");
    } else if (!launch.error) reportedErrors.delete(launch.gameId);
    if (launch.compatibilityLogError && reportedLogErrors.get(launch.gameId) !== launch.compatibilityLogError) {
      reportedLogErrors.set(launch.gameId, launch.compatibilityLogError);
      showToast(launch.compatibilityLogError, "error");
    } else if (!launch.compatibilityLogError) reportedLogErrors.delete(launch.gameId);
    if (launch.status === "idle") {
      hiddenForSession.delete(launch.gameId);
      if (restoreOnExit.delete(launch.gameId)) {
        void showWindow().catch((error) => launchError.set(toMessage(error)));
      }
    } else if (launch.status === "running" && !hiddenForSession.has(launch.gameId)) {
      hiddenForSession.add(launch.gameId);
      if (get(settings).data.hideOnGameStart && get(appInfo).data.trayAvailable) {
        restoreOnExit.add(launch.gameId);
        void hideWindow().catch((error) => {
          restoreOnExit.delete(launch.gameId);
          launchError.set(toMessage(error));
        });
      }
    }
  }
});

function configureLaunchPolling(): void {
  if (pollTimer !== null) clearInterval(pollTimer);
  pollTimer = get(hasPendingLaunch) && (!eventsReady || get(windowActive))
    ? setInterval(() => void launchStates.load(), eventsReady ? 30000 : 2000)
    : null;
}

hasPendingLaunch.subscribe(configureLaunchPolling);
windowActive.subscribe((active) => {
  if (active && eventsReady) void launchStates.load();
  configureLaunchPolling();
});

export function startLaunchEvents(): Promise<void> {
  listening ??= (async () => {
    try {
      await onGameLaunchStates((states) => launchStates.set(states));
      eventsReady = true;
      configureLaunchPolling();
    } catch (error) {
      console.warn("Game activity events unavailable; polling remains enabled", error);
    }
    try {
      await onShortcutLaunchFailure((message) => launchError.set(message));
    } catch (error) {
      console.warn("Shortcut failure events unavailable", error);
    }
  })();
  return listening;
}

export async function playGame(game: Game): Promise<void> {
  launchError.set(null);
  pendingGameId.set(game.id);
  try {
    if (game.steamInstallPath !== null) {
      const inspection = await inspectSteamGameLaunch(game.id);
      if (inspection.status === "mismatch" || (inspection.status === "unknown" && inspection.steamRunning)) {
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
        throw new Error(t("L'avvio di giochi manuali non è disponibile su questa piattaforma.", undefined));
      }
    } else {
      throw new Error(t("Seleziona un eseguibile nelle impostazioni del gioco.", undefined));
    }
    await launchStates.load();
  } catch (error) {
    let message = toMessage(error);
    if (game.steamInstallPath === null && game.executablePath !== null && get(appInfo).data.platform === "linux") {
      try {
        message += t(" Log compatibilità: {0}", undefined, [await getCompatibilityLogsDirectory()]);
      } catch (diagnosticsError) {
        message += t(" Impossibile trovare i log di compatibilità: {0}", undefined, [toMessage(diagnosticsError)]);
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
