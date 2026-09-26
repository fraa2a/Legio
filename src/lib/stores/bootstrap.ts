import { appInfo } from "./app-info";
import { settings } from "./settings";
import { games } from "./games";
import { downloads } from "./downloads";
import { source, refreshSource } from "./source";
import { checkConnectivity, network } from "./network";
import { launchStates } from "./launch";

export async function hydrateApp(): Promise<void> {
  await Promise.all([
    appInfo.load(),
    settings.load(),
    games.load(),
    downloads.load(),
    source.load(),
    network.load(),
    launchStates.load(),
  ]);
  await Promise.all([checkConnectivity(), refreshSource()]);
}
