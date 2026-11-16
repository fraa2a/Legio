import { t, type Language } from "../../i18n";
import type { PlaytimeRecap } from "../../services/playtime";
import { formatPlaytime } from "../home/home-model";
import { formatRecapDate } from "./recap-model";

export type RecapTileKey = "total" | "sessions" | "games" | "day" | "session" | "streak" | "span";

export interface RecapTile {
  key: RecapTileKey;
  label: string;
  value: string;
  note: string | null;
}

const shortDate: Intl.DateTimeFormatOptions = { day: "numeric", month: "short" };

export function formatStreakDays(days: number, language: Language): string {
  return days === 1 ? t("1 giorno", language) : t("{0} giorni", language, [days]);
}

export function recapTiles(recap: PlaytimeRecap, locale: string, language: Language): RecapTile[] {
  const { longestDay, longestSession, longestStreak, firstPlayed, lastPlayed } = recap;
  return [
    { key: "total", label: t("Tempo di gioco", language), value: formatPlaytime(recap.totalMilliseconds), note: null },
    { key: "sessions", label: t("Sessioni", language), value: String(recap.sessionCount), note: null },
    { key: "games", label: t("Giochi giocati", language), value: String(recap.gameCount), note: null },
    {
      key: "day",
      label: t("Giorno più lungo", language),
      value: longestDay === null ? "-" : formatPlaytime(longestDay.milliseconds),
      note: longestDay === null ? null : formatRecapDate(longestDay.date, locale),
    },
    {
      key: "session",
      label: t("Sessione più lunga", language),
      value: longestSession === null ? "-" : formatPlaytime(longestSession.milliseconds),
      note: longestSession === null ? null : `${longestSession.name}, ${formatRecapDate(longestSession.date, locale, shortDate)}`,
    },
    {
      key: "streak",
      label: t("Serie di giorni", language),
      value: longestStreak === null ? "-" : formatStreakDays(longestStreak.days, language),
      note: longestStreak === null ? null : `${formatRecapDate(longestStreak.start, locale, shortDate)} - ${formatRecapDate(longestStreak.end, locale, shortDate)}`,
    },
    {
      key: "span",
      label: t("Primo e ultimo giorno", language),
      value: firstPlayed === null ? "-" : formatRecapDate(firstPlayed, locale, shortDate),
      note: lastPlayed === null ? null : t("Ultimo: {0}", language, [formatRecapDate(lastPlayed, locale, shortDate)]),
    },
  ];
}
