import { invoke } from "@tauri-apps/api/core";

export type NetworkStatus = "unknown" | "online";

export interface ConnectivityCheck {
  status: NetworkStatus;
  detail: string | null;
}

export interface NetworkLogStatus {
  directory: string | null;
  lastError: string | null;
  droppedRecords: number;
  pendingRecords: number;
}

export function getNetworkStatus(): Promise<NetworkStatus> {
  return invoke<NetworkStatus>("get_network_status");
}

export function checkSteamConnectivity(): Promise<ConnectivityCheck> {
  return invoke<ConnectivityCheck>("check_steam_connectivity");
}

export function getNetworkLogStatus(): Promise<NetworkLogStatus> {
  return invoke<NetworkLogStatus>("get_network_log_status");
}
