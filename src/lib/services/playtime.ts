import { save } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
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

export interface RecapTopGame {
  gameId: string;
  name: string;
  milliseconds: number;
  share: number;
}

export interface RecapDay {
  date: string;
  milliseconds: number;
}

export interface RecapSession extends RecapDay {
  gameId: string;
  name: string;
}

export interface RecapStreak {
  start: string;
  end: string;
  days: number;
}

export interface PlaytimeRecap {
  totalMilliseconds: number;
  sessionCount: number;
  gameCount: number;
  topGames: RecapTopGame[];
  longestDay: RecapDay | null;
  longestSession: RecapSession | null;
  longestStreak: RecapStreak | null;
  // Monday first.
  byWeekday: number[];
  // Twelve months for a year, one entry per day for a month.
  byBucket: number[];
  firstPlayed: string | null;
  lastPlayed: string | null;
}

export function getPlaytimeRecap(year: number, month: number | null, dayBoundaries: number[]): Promise<PlaytimeRecap> {
  return invoke<PlaytimeRecap>("get_playtime_recap", { year, month, dayBoundaries });
}

export function getPlaytimeRecapYears(): Promise<number[]> {
  return invoke<number[]>("get_playtime_recap_years");
}

// Returns false when the person closes the save dialog.
export async function exportPlaytimeRecapImage(pngBase64: string, defaultName: string): Promise<boolean> {
  const path = await save({ title: t("Salva scheda", undefined), defaultPath: defaultName, filters: [{ name: "PNG", extensions: ["png"] }] });
  if (path === null) return false;
  await invoke<void>("save_playtime_recap_image", { path, pngBase64 });
  return true;
}

export function setGamePlaytime(gameId: string, totalMilliseconds: number): Promise<void> {
  return invoke<void>("set_game_playtime", { gameId, totalMilliseconds });
}
