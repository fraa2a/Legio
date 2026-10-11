import { get } from "svelte/store";
import { language } from "../i18n";
import { invoke } from "./invoke";
import type { ArtworkWork } from "./artwork-work";

export interface SteamDetails {
  steamAppId: number;
  name: string;
  appType: string;
  shortDescription: string | null;
  detailedDescription: string | null;
  systemRequirements?: { minimum: string | null; recommended: string | null } | null;
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
  kind: "cancelled" | "invalid_app_id" | "unavailable" | "invalid_response" | "timeout" | "network" | "http" | "too_large" | "database" | "internal";
  message: string;
  status: number | null;
}

export type DetailsPolicy = "cache_or_fetch" | "refresh_if_expired" | "force_refresh";
const pending = new Map<string, Promise<SteamDetailsResult>>();
export function getSteamDetails(steamAppId: number, policy: DetailsPolicy, work: ArtworkWork): Promise<SteamDetailsResult> {
  const locale = get(language);
  const key = JSON.stringify([steamAppId, locale, policy, work.requestId]);
  const existing = pending.get(key);
  if (existing) return existing;
  const result = invoke<SteamDetailsResult>("get_steam_details", { steamAppId, policy, language: locale, requestId: work.requestId, origin: work.origin }).finally(() => pending.delete(key));
  pending.set(key, result);
  return result;
}
