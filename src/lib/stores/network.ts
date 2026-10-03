import { t, language } from "../i18n";
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

export const networkSummary = derived([network, connectivity, language], ([state, check, selected]) => {
  if (state.data === "online") {
    return { status: "Online", steam: t("Raggiungibile", selected), detail: null };
  }
  if (check === null) {
    return { status: t("Non verificato", selected), steam: t("Non verificato", selected), detail: null };
  }
  return { status: t("Non raggiungibile", selected), steam: t("Non raggiungibile", selected), detail: check.detail };
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
