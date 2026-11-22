import { derived, writable } from "svelte/store";
import { getModLinkHandler, observeModLinks, takeModLinks, type LinkHandler, type ModLinkRequest } from "../services/mod-links";
import { toMessage } from "../utils/errors";
import { updateSettings } from "./settings";
import { showToast } from "./toast";

const queue = writable<readonly ModLinkRequest[]>([]);

// Links are handled one at a time, in the order they arrived.
export const currentModLink = derived(queue, (requests) => requests[0] ?? null);

export function dismissModLink(): void {
  queue.update((requests) => requests.slice(1));
}

let starting: Promise<void> | null = null;
let draining = false;
let requested = false;

async function drain(): Promise<void> {
  requested = true;
  if (draining) return;
  draining = true;
  try {
    while (requested) {
      requested = false;
      const pending = await takeModLinks();
      if (pending.length > 0) queue.update((requests) => [...requests, ...pending]);
    }
  } catch (error) {
    showToast(toMessage(error), "error");
  } finally {
    draining = false;
  }
}

export function startModLinks(): Promise<void> {
  if (starting !== null) return starting;
  starting = (async () => {
    await observeModLinks(() => { void drain(); });
    await drain();
  })().catch((error) => {
    starting = null;
    showToast(toMessage(error), "error");
  });
  return starting;
}

// Turning the option off is always saved. The returned handler is who still opens the links, so the page can say so.
export async function setModLinksEnabled(enabled: boolean): Promise<LinkHandler | null> {
  await updateSettings({ thunderstoreLinksEnabled: enabled });
  return enabled ? null : getModLinkHandler();
}
