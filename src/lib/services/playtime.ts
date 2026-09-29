import { invoke } from "@tauri-apps/api/core";

export interface PlaytimeSummary {
  gameId: string;
  totalMilliseconds: number;
  activeSessions: number;
  lastPlayedAt: number | null;
}

export function getPlaytimeSummaries(): Promise<PlaytimeSummary[]> {
  return invoke<PlaytimeSummary[]>("get_playtime_summaries");
}
