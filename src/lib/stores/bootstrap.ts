import { appInfo } from "./app-info";
import { settings } from "./settings";
import { games } from "./games";
import { bandwidthLimit, downloads } from "./downloads";
import { source, refreshSource } from "./source";
import { checkConnectivity, network } from "./network";
import { launchStates } from "./launch";
import { importSteamLibrary } from "./steam-library";
import { get } from "svelte/store";

let steamScanTimer: ReturnType<typeof setInterval> | null = null;

export function configureSteamScanInterval(minutes: number): void {
  if (steamScanTimer !== null) clearInterval(steamScanTimer);
  steamScanTimer = setInterval(() => void importSteamLibrary(), minutes * 60 * 1000);
}

export async function hydrateApp(): Promise<void> {
  await Promise.all([
    appInfo.load(),
    settings.load(),
    games.load(),
    downloads.load(),
    bandwidthLimit.load(),
    source.load(),
    network.load(),
    launchStates.load(),
  ]);
  if (steamScanTimer === null) {
    void importSteamLibrary();
    configureSteamScanInterval(get(settings).data.steamLibraryPollMinutes);
  }
  await Promise.all([checkConnectivity(), refreshSource()]);
}
