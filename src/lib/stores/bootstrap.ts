import { listenGameUpdatesChanged, listenDownloadsChanged } from "../services/game-updates";
import { gameUpdates } from "./game-updates";
import { appInfo } from "./app-info";
import { settings } from "./settings";
import { games } from "./games";
import { bandwidthLimit, downloads, installedFolder, startDownloadProgressPolling } from "./downloads";
import { startQueueShutdown } from "./queue-shutdown";
import { startSourceLinks } from "./source-links";
import { source, refreshSource } from "./source";
import { checkConnectivity, network } from "./network";
import { hasPendingLaunch, launchError, launchStates, startLaunchEvents } from "./launch";
import { playtime } from "./playtime";
import { importSteamLibrary } from "./steam-library";
import { get } from "svelte/store";
import { activeSection, openGame } from "./navigation";

let steamScanTimer: ReturnType<typeof setInterval> | null = null;

export function configureSteamScanInterval(minutes: number): void {
  if (steamScanTimer !== null) clearInterval(steamScanTimer);
  steamScanTimer = null;
  if (!get(settings).data.syncSteamGames) return;
  steamScanTimer = setInterval(() => {
    if (get(settings).data.syncSteamGames && !get(hasPendingLaunch)) void importSteamLibrary();
  }, minutes * 60 * 1000);
}

export async function hydrateApp(): Promise<void> {
  await startLaunchEvents();
  if (get(settings).status !== "ready") await settings.load();
  await appInfo.load();
  await Promise.all([
    games.load(),
    downloads.load(),
    bandwidthLimit.load(),
    installedFolder.load(),
    source.load(),
    gameUpdates.load(),
    network.load(),
    launchStates.load(),
    playtime.load(),
  ]);
  startDownloadProgressPolling();
  await startQueueShutdown();
  await listenDownloadsChanged(() => { void downloads.reload(); });
  await listenGameUpdatesChanged(() => { void gameUpdates.reload(); void source.reload(); });
  if (get(settings).data.launchInLibrary) activeSection.set("library");
  const startup = get(appInfo).data;
  if (startup.startupLaunchGameId !== null) {
    openGame(startup.startupLaunchGameId);
  }
  if (startup.startupLaunchError !== null) launchError.set(startup.startupLaunchError);
  if (steamScanTimer === null) {
    configureSteamScanInterval(get(settings).data.steamLibraryPollMinutes);
    if (get(settings).data.syncSteamGames && !get(hasPendingLaunch)) await importSteamLibrary();
  }
  await startSourceLinks();
  await Promise.all([checkConnectivity(), refreshSource()]);
}
