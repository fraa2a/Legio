import { writable } from "svelte/store";

export const toast = writable<{ id: number; message: string } | null>(null);

let nextId = 0;
let timeout: ReturnType<typeof setTimeout> | undefined;

export function showToast(message: string): void {
  if (timeout !== undefined) clearTimeout(timeout);
  toast.set({ id: ++nextId, message });
  timeout = setTimeout(() => {
    toast.set(null);
    timeout = undefined;
  }, 1600);
}
