import { derived, writable } from "svelte/store";
import { unsyncedCommunities } from "../features/modding/mod-link-model";
import { observeModLinks, resolveModLink, takeModLinks, type ModLinkRequest, type ModLinkResolution } from "../services/mod-links";
import { toMessage } from "../utils/errors";
import { ensureCatalog } from "./modding";
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

// A game whose catalog was never downloaded cannot say whether it has the mod, so those are synced once first.
export async function resolveLink(fullName: string, version: string): Promise<ModLinkResolution> {
  const resolution = await resolveModLink(fullName, version);
  const missing = unsyncedCommunities(resolution);
  if (missing.length === 0) return resolution;
  await Promise.all(missing.map(ensureCatalog));
  return resolveModLink(fullName, version);
}
