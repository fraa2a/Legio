import { checkSteamConnectivity, getNetworkStatus, type NetworkStatus } from "../services/network";
import { createResource } from "./resource";

export const network = createResource<NetworkStatus>("unknown", getNetworkStatus);

export async function checkConnectivity(): Promise<void> {
  try {
    network.set((await checkSteamConnectivity()).status);
  } catch {
    // invoke() already logged the failure; the last known status stays.
  }
}
