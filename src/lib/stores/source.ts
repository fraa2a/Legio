import { addDownloadSource, getLegioSource, refreshLegioSource, removeDownloadSource, type SourceSnapshot } from "../services/legio-source";
import { toMessage } from "../utils/errors";
import { writable } from "svelte/store";
import { createResource } from "./resource";
import { invalidateCatalogAvailability } from "../services/catalog";

export const source = createResource<SourceSnapshot>(
  { manifest: null, cachedAt: null, stale: false, warning: null, sources: [] },
  getLegioSource,
  (snapshot) => snapshot.sources.length === 0,
);

export const sourceRefreshError = writable<string | null>(null);
export const sourceActionError = writable<string | null>(null);
export const sourceBusy = writable(false);
let previousManifest: SourceSnapshot["manifest"] = null;
source.subscribe((snapshot) => {
  if (snapshot.data.manifest !== previousManifest) {
    previousManifest = snapshot.data.manifest;
    invalidateCatalogAvailability();
  }
});

let pending: Promise<unknown> = Promise.resolve();

function changeSource(action: () => Promise<SourceSnapshot>, errorStore: typeof sourceActionError): Promise<boolean> {
  const result = pending.then(async () => {
    sourceBusy.set(true);
    errorStore.set(null);
    try {
      source.set(await action());
      sourceRefreshError.set(null);
      return true;
    } catch (error) {
      errorStore.set(toMessage(error));
      return false;
    } finally {
      sourceBusy.set(false);
    }
  });
  pending = result;
  return result;
}

export function addSource(url: string): Promise<boolean> {
  return changeSource(() => addDownloadSource(url), sourceActionError);
}

export function removeSource(id: string): Promise<boolean> {
  return changeSource(() => removeDownloadSource(id), sourceActionError);
}

export async function refreshSource(): Promise<void> {
  await changeSource(refreshLegioSource, sourceRefreshError);
}
