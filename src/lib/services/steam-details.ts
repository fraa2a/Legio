import { invoke } from "@tauri-apps/api/core";

export interface SteamDetails {
  steamAppId: number;
  name: string;
  appType: string;
  shortDescription: string | null;
  developers: string[];
  publishers: string[];
  genres: string[];
  platforms: { windows: boolean; mac: boolean; linux: boolean } | null;
  releaseDate: { comingSoon: boolean; date: string } | null;
  assets: {
    header: string | null;
    capsule: string | null;
    background: string | null;
    screenshots: { thumbnail: string | null; full: string | null }[];
  };
}

export interface SteamDetailsResult {
  details: SteamDetails | null;
  cachedAt: number | null;
  stale: boolean;
}

export interface SteamDetailsError {
  kind: "invalid_app_id" | "unavailable" | "invalid_response" | "timeout" | "network" | "http" | "too_large" | "database" | "internal";
  message: string;
  status: number | null;
}

export function getSteamDetails(steamAppId: number, refresh: boolean): Promise<SteamDetailsResult> {
  return invoke<SteamDetailsResult>("get_steam_details", { steamAppId, refresh });
}

export type SteamAssetKind = "header" | "capsule" | "screenshot";

export interface SteamAsset {
  bytes: number[];
  contentType: string;
  stale: boolean;
  cacheWarning: string | null;
}

export function getSteamAsset(steamAppId: number, asset: SteamAssetKind, index?: number): Promise<SteamAsset> {
  return invoke<SteamAsset>("get_steam_asset", { steamAppId, asset, index });
}
