import type { Game } from "../../services/local-state";
import type { PlaytimeSummary } from "../../services/playtime";

export function recentGames(games: Game[], summaries: PlaytimeSummary[]) {
  const byId = new Map(summaries.map((summary) => [summary.gameId, summary]));
  const seen = new Set<string>();
  return games
    .flatMap((game) => {
      const summary = byId.get(game.id);
      return summary?.lastPlayedAt == null ? [] : [{ game, summary }];
    })
    .sort((left, right) => (right.summary.lastPlayedAt ?? 0) - (left.summary.lastPlayedAt ?? 0))
    .filter(({ game }) => {
      const key = game.steamAppId === null ? game.id : `steam:${game.steamAppId}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

export function monthCalendar(year: number, month: number) {
  const first = new Date(year, month, 1);
  const days = new Date(year, month + 1, 0).getDate();
  const offset = (first.getDay() + 6) % 7;
  const boundaries = Array.from({ length: days + 1 }, (_, day) => new Date(year, month, day + 1).getTime());
  const cells = Array.from({ length: Math.max(5, Math.ceil((offset + days) / 7)) * 7 }, (_, index) => {
    const day = index - offset + 1;
    return day > 0 && day <= days ? day : null;
  });
  return { first, boundaries, cells };
}

export function formatPlaytime(milliseconds: number): string {
  const minutes = Math.floor(Math.max(0, milliseconds) / 60000);
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h${minutes % 60 === 0 ? "" : ` ${minutes % 60} min`}`;
}

export function activityIntensity(milliseconds: number): number {
  return Math.min(1, Math.max(0, milliseconds / (8 * 3600000)));
}
