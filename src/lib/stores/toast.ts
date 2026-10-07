import { writable } from "svelte/store";

export type ToastTone = "info" | "success" | "error";
export interface Toast { id: number; message: string; tone: ToastTone; onRetry?: () => void; retryLabel?: string }
export const toast = writable<Toast[]>([]);

let nextId = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export function dismissToast(id: number): void {
  clearTimeout(timers.get(id));
  timers.delete(id);
  toast.update((notices) => notices.filter((notice) => notice.id !== id));
}

export function showToast(message: string, tone: ToastTone = "info", action?: { onRetry?: () => void; retryLabel?: string }): void {
  const id = ++nextId;
  let duplicate = false;
  toast.update((notices) => {
    duplicate = notices.some((notice) => notice.message === message && notice.tone === tone);
    return duplicate ? notices : [...notices, { id, message, tone, ...action }];
  });
  if (duplicate) return;
  timers.set(id, setTimeout(() => dismissToast(id), 5000));
}
