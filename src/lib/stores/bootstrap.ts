import { appInfo } from "./app-info";
import { settings } from "./settings";
import { games } from "./games";
import { startLibraryHeroCaching } from "./library-artwork";
import { bandwidthLimit, downloads, installedFolder, startDownloadProgressPolling } from "./downloads";
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
  steamScanTimer = setInterval(() => {
    if (!get(hasPendingLaunch)) void importSteamLibrary();
  }, minutes * 60 * 1000);
}

export async function hydrateApp(): Promise<void> {
  await startLaunchEvents();
  if (get(settings).status !== "ready") await settings.load();
  await Promise.all([
    appInfo.load(),
    games.load(),
    downloads.load(),
    bandwidthLimit.load(),
    installedFolder.load(),
    source.load(),
    network.load(),
    launchStates.load(),
    playtime.load(),
  ]);
  startLibraryHeroCaching();
  startDownloadProgressPolling();
  if (get(settings).data.launchInLibrary) activeSection.set("library");
  const startup = get(appInfo).data;
  if (startup.startupLaunchGameId !== null) {
    openGame(startup.startupLaunchGameId);
    if (startup.startupLaunchError !== null) launchError.set(startup.startupLaunchError);
  }
  if (steamScanTimer === null) {
    configureSteamScanInterval(get(settings).data.steamLibraryPollMinutes);
    if (!get(hasPendingLaunch)) await importSteamLibrary();
  }
  await Promise.all([checkConnectivity(), refreshSource()]);
}
