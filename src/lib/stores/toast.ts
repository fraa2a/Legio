import { get, writable } from "svelte/store";
import { recordNotification, retryNotification, type NotificationTone } from "./notifications";

export type ToastTone = NotificationTone;
export interface Toast { id: number; message: string; tone: ToastTone; detail?: string; onRetry?: () => void; retryLabel?: string }
export const toast = writable<Toast[]>([]);

let nextId = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function dismissToast(id: number): void {
  clearTimeout(timers.get(id));
  timers.delete(id);
  toast.update((notices) => notices.filter((notice) => notice.id !== id));
}

// Every shown toast is also kept in the notification history, with its technical detail.
export function showToast(message: string, tone: ToastTone = "info", action?: { onRetry?: () => void; retryLabel?: string; persistent?: boolean; detail?: string }): void {
  if (get(toast).some((notice) => notice.message === message && notice.tone === tone)) return;
  const id = ++nextId;
  const retry = action?.onRetry;
  const entryId = recordNotification(message, tone, retry === undefined ? undefined : { run: retry, label: action?.retryLabel }, Date.now(), action?.detail);
  const onRetry = retry === undefined ? undefined : () => void retryNotification(entryId);
  toast.update((notices) => [...notices, { id, message, tone, detail: action?.detail, onRetry, retryLabel: action?.retryLabel }]);
  // A technical detail stays until dismissed so it can be read and copied.
  if (action?.persistent || action?.detail !== undefined) return;
  timers.set(id, setTimeout(() => dismissToast(id), 5000));
}
