import {
  checkSteamConnectivity,
  getNetworkStatus,
  type ConnectivityCheck,
  type NetworkStatus,
} from "../services/network";
import { toMessage } from "../utils/errors";
import { writable } from "svelte/store";
import { createResource } from "./resource";

export const network = createResource<NetworkStatus>("unknown", getNetworkStatus);

const connectivity = writable<ConnectivityCheck | null>(null);

export const connectivityError = writable<string | null>(null);

export async function checkConnectivity(): Promise<void> {
  connectivityError.set(null);
  try {
    const check = await checkSteamConnectivity();
    connectivity.set(check);
    network.set(check.status);
  } catch (error) {
    connectivityError.set(toMessage(error));
  }
}
