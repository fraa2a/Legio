import type { Snippet } from "svelte";
import { writable, type Readable } from "svelte/store";

// Page controls shown in the title bar: `title` replaces the page title,
// `start` sits after the back button and `end` before the window controls.
export interface TitleBarContent {
  title: string | null;
  start?: Snippet;
  end?: Snippet;
}

const content = writable<TitleBarContent | null>(null);

export const titleBarContent: Readable<TitleBarContent | null> = { subscribe: content.subscribe };

// Returns the cleanup that removes the content, unless a newer one replaced it.
export function showTitleBarContent(next: TitleBarContent): () => void {
  content.set(next);
  return () => content.update((current) => (current === next ? null : current));
}
