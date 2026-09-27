import { invoke } from "@tauri-apps/api/core";

export interface PlaytimeSummary {
  gameId: string;
  totalMilliseconds: number;
  activeSessions: number;
}

export function getPlaytimeSummaries(): Promise<PlaytimeSummary[]> {
  return invoke<PlaytimeSummary[]>("get_playtime_summaries");
}
