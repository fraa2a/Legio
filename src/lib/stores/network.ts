import {
  checkSteamConnectivity,
  getNetworkStatus,
  type ConnectivityCheck,
  type NetworkStatus,
} from "../services/network";
import { toMessage } from "../utils/errors";
import { derived, writable } from "svelte/store";
import { createResource } from "./resource";

export const network = createResource<NetworkStatus>("unknown", getNetworkStatus);

const connectivity = writable<ConnectivityCheck | null>(null);

export const connectivityError = writable<string | null>(null);

export const networkSummary = derived([network, connectivity], ([state, check]) => {
  if (state.data === "online") {
    return { status: "Online", steam: "Raggiungibile", detail: null };
  }
  if (check === null) {
    return { status: "Non verificato", steam: "Non verificato", detail: null };
  }
  return { status: "Non raggiungibile", steam: "Non raggiungibile", detail: check.detail };
});

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
