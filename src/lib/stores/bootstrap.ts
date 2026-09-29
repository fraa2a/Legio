import { appInfo } from "./app-info";
import { settings } from "./settings";
import { games } from "./games";
import { bandwidthLimit, downloads } from "./downloads";
import { source, refreshSource } from "./source";
import { checkConnectivity, network } from "./network";
import { launchError, launchStates } from "./launch";
import { playtime } from "./playtime";
import { importSteamLibrary } from "./steam-library";
import { get } from "svelte/store";
import { activeSection, openGame } from "./navigation";

let steamScanTimer: ReturnType<typeof setInterval> | null = null;

export function configureSteamScanInterval(minutes: number): void {
  if (steamScanTimer !== null) clearInterval(steamScanTimer);
  steamScanTimer = setInterval(() => void importSteamLibrary(), minutes * 60 * 1000);
}

export async function hydrateApp(): Promise<void> {
  await settings.load();
  await Promise.all([
    appInfo.load(),
    games.load(),
    downloads.load(),
    bandwidthLimit.load(),
    source.load(),
    network.load(),
    launchStates.load(),
    playtime.load(),
  ]);
  if (get(settings).data.launchInLibrary) activeSection.set("library");
  const startup = get(appInfo).data;
  if (startup.startupLaunchGameId !== null) {
    openGame(startup.startupLaunchGameId);
    if (startup.startupLaunchError !== null) launchError.set(startup.startupLaunchError);
  }
  if (steamScanTimer === null) {
    configureSteamScanInterval(get(settings).data.steamLibraryPollMinutes);
    await importSteamLibrary();
  }
  await Promise.all([checkConnectivity(), refreshSource()]);
}
