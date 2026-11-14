import { invoke } from "./invoke";

export type ProtonDbTier = "platinum" | "gold" | "silver" | "bronze" | "borked" | "pending" | "unknown";
export type ProtonDbConfidence = "strong" | "good" | "moderate" | "low" | "inadequate" | "unknown";

export interface ProtonDbSummary {
  tier: ProtonDbTier;
  confidence: ProtonDbConfidence | null;
  score: number | null;
  totalReports: number;
  trendingTier: ProtonDbTier | null;
  bestReportedTier: ProtonDbTier | null;
}

export interface ProtonDbResult {
  // Null when ProtonDB has no reports for the game.
  summary: ProtonDbSummary | null;
  cachedAt: number | null;
  stale: boolean;
}

export const getProtonDbSummary = (steamAppId: number) =>
  invoke<ProtonDbResult>("get_protondb_summary", { steamAppId });
