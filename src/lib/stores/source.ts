import { getLegioSource, refreshLegioSource, type SourceSnapshot } from "../services/legio-source";
import { toMessage } from "../utils/errors";
import { writable } from "svelte/store";
import { createResource } from "./resource";
import { invalidateCatalogAvailability } from "../services/catalog";

export const source = createResource<SourceSnapshot>(
  { manifest: null, cachedAt: null, stale: false, warning: null },
  getLegioSource,
  (snapshot) => snapshot.manifest === null,
);

export const sourceRefreshError = writable<string | null>(null);
let previousManifest: SourceSnapshot["manifest"] = null;
source.subscribe((snapshot) => {
  if (snapshot.data.manifest !== previousManifest) {
    previousManifest = snapshot.data.manifest;
    invalidateCatalogAvailability();
  }
});

export async function refreshSource(): Promise<void> {
  sourceRefreshError.set(null);
  try {
    source.set(await refreshLegioSource());
  } catch (error) {
    sourceRefreshError.set(toMessage(error));
  }
}
