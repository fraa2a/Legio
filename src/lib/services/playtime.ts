import { invoke } from "./invoke";

export interface PlaytimeSummary {
  gameId: string;
  totalMilliseconds: number;
  activeSessions: number;
  lastPlayedAt: number | null;
}

export function getPlaytimeSummaries(): Promise<PlaytimeSummary[]> {
  return invoke<PlaytimeSummary[]>("get_playtime_summaries");
}

export function getPlaytimeActivity(dayBoundaries: number[]): Promise<number[]> {
  return invoke<number[]>("get_playtime_activity", { dayBoundaries });
}
