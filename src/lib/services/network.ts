import { invoke } from "@tauri-apps/api/core";

export type NetworkStatus = "unknown" | "online";

export interface ConnectivityCheck {
  status: NetworkStatus;
  detail: string | null;
}

export function getNetworkStatus(): Promise<NetworkStatus> {
  return invoke<NetworkStatus>("get_network_status");
}

export function checkSteamConnectivity(): Promise<ConnectivityCheck> {
  return invoke<ConnectivityCheck>("check_steam_connectivity");
}
