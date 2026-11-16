import { derived, writable, type Readable } from "svelte/store";

export type NotificationTone = "info" | "success" | "warning" | "error";

export interface NotificationEntry {
  id: number;
  message: string;
  tone: NotificationTone;
  // Technical detail of a failure, kept so it can still be read and copied later.
  detail: string | null;
  // Epoch milliseconds.
  time: number;
  // Cleared once used, so a retry runs at most once from the toast or the history.
  retry: { run: () => void; label?: string } | null;
}

interface History {
  entries: NotificationEntry[];
  // Entries with a larger id are unread.
  readUpTo: number;
}

// The history lives in memory for the session only.
export const notificationLimit = 100;

const history = writable<History>({ entries: [], readUpTo: 0 });
let nextId = 0;

// Newest first.
export const notifications: Readable<NotificationEntry[]> = derived(history, ($history) => $history.entries);

export const readNotificationsUpTo: Readable<number> = derived(history, ($history) => $history.readUpTo);

// Errors and warnings are counted apart from the rest so the badge can stand out.
export const unreadNotifications: Readable<{ attention: number; other: number }> = derived(history, ($history) => {
  let attention = 0;
  let other = 0;
  for (const entry of $history.entries) {
    if (entry.id <= $history.readUpTo) break;
    if (entry.tone === "error" || entry.tone === "warning") attention++;
    else other++;
  }
  return { attention, other };
});

export function recordNotification(
  message: string,
  tone: NotificationTone,
  retry?: { run: () => void; label?: string },
  time = Date.now(),
  detail?: string,
): number {
  const id = ++nextId;
  const entry: NotificationEntry = { id, message, tone, detail: detail ?? null, time, retry: retry ?? null };
  history.update(({ entries, readUpTo }) => ({ entries: [entry, ...entries].slice(0, notificationLimit), readUpTo }));
  return id;
}

// Runs the retry of an entry once; later calls and evicted entries do nothing.
export function retryNotification(id: number): boolean {
  let retry = null as NotificationEntry["retry"];
  history.update(({ entries, readUpTo }) => ({
    readUpTo,
    entries: entries.map((entry) => {
      if (entry.id !== id || entry.retry === null) return entry;
      retry = entry.retry;
      return { ...entry, retry: null };
    }),
  }));
  if (retry === null) return false;
  retry.run();
  return true;
}

// Called when the action behind a retry no longer applies, such as a download retried elsewhere.
export function dropNotificationRetry(id: number): void {
  history.update(({ entries, readUpTo }) => ({
    readUpTo,
    entries: entries.map((entry) => (entry.id === id && entry.retry !== null ? { ...entry, retry: null } : entry)),
  }));
}

export function markNotificationsRead(): void {
  history.update(({ entries }) => ({ entries, readUpTo: nextId }));
}

export function clearNotifications(): void {
  history.set({ entries: [], readUpTo: nextId });
}
