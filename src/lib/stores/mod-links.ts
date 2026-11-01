import { derived, writable } from "svelte/store";
import { catalogRefreshes } from "../features/modding/mod-link-model";
import { getModLinkHandler, observeModLinks, resolveModLink, takeModLinks, type LinkHandler, type ModLinkRequest, type ModLinkResolution } from "../services/mod-links";
import { toMessage } from "../utils/errors";
import { refreshCatalog } from "./modding";
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

// A catalog that was never downloaded, or lacks the linked version, may just be old: it is refreshed once before giving up.
export async function resolveLink(fullName: string, version: string): Promise<ModLinkResolution> {
  const resolution = await resolveModLink(fullName, version);
  const refreshes = catalogRefreshes(resolution);
  if (refreshes.length === 0) return resolution;
  await Promise.all(refreshes.map(({ community, force }) => refreshCatalog(community, force)));
  return resolveModLink(fullName, version);
}

// Turning the option off is always saved. The returned handler is who still opens the links, so the page can say so.
export async function setModLinksEnabled(enabled: boolean): Promise<LinkHandler | null> {
  await updateSettings({ thunderstoreLinksEnabled: enabled });
  return enabled ? null : getModLinkHandler();
}
