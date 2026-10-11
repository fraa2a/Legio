import { language } from "../i18n";
import { get, writable } from "svelte/store";
import {
  getSteamDetails,
  type SteamDetails,
  type SteamDetailsResult,
} from "../services/steam-metadata";
import { toMessage } from "../utils/errors";
import { createArtworkWork, isArtworkCancelled, type ArtworkWork } from "../services/artwork-work";
import type { DetailsPolicy } from "../services/steam-metadata";
import type { LoadStatus } from "./resource";

export interface SteamDetailsState {
  status: LoadStatus;
  details: SteamDetails | null;
  cachedAt: number | null;
  stale: boolean;
  error: string | null;
}

const idle: SteamDetailsState = {
  status: "idle",
  details: null,
  cachedAt: null,
  stale: false,
  error: null,
};

export const steamDetails = writable<Record<number, SteamDetailsState>>({});

const generations = new Map<number, number>();
const inflight = new Map<number, Promise<SteamDetailsResult>>();

language.subscribe(() => {
  for (const [id, generation] of generations) generations.set(id, generation + 1);
  inflight.clear();
  steamDetails.set({});
});

function patch(steamAppId: number, values: Partial<SteamDetailsState>): void {
  steamDetails.update((all) => ({ ...all, [steamAppId]: { ...(all[steamAppId] ?? idle), ...values } }));
}

export function loadSteamDetails(
  steamAppId: number,
  policy: DetailsPolicy | boolean,
  suppliedWork?: ArtworkWork,
): Promise<SteamDetailsResult> {
  const actualPolicy = typeof policy === "boolean" ? (policy ? "force_refresh" : "cache_or_fetch") : policy;
  if (actualPolicy !== "force_refresh") {
    const pending = inflight.get(steamAppId);
    if (pending !== undefined) return pending;
  }
  const generation = (generations.get(steamAppId) ?? 0) + 1;
  generations.set(steamAppId, generation);
  const work = suppliedWork ?? createArtworkWork(steamAppId, "visible");
  const request = (async () => {
    const previous = get(steamDetails)[steamAppId];
    patch(steamAppId, { status: previous?.details ? "ready" : "loading", error: null });
    try {
      const result = await getSteamDetails(steamAppId, actualPolicy, work);
      if (generations.get(steamAppId) === generation) patch(steamAppId, {
        status: result.details === null ? "empty" : "ready",
        details: result.details,
        cachedAt: result.cachedAt,
        stale: result.stale,
        error: null,
      });
      return result;
    } catch (error) {
      if (generations.get(steamAppId) === generation && !isArtworkCancelled(error)) {
        const previous = get(steamDetails)[steamAppId];
        patch(steamAppId, { status: previous?.details ? "ready" : "error", stale: true, error: toMessage(error) });
      }
      throw error;
    } finally {
      if (!suppliedWork) work.release();
      if (generations.get(steamAppId) === generation) inflight.delete(steamAppId);
    }
  })();
  inflight.set(steamAppId, request);
  return request;
}

export function ensureSteamDetails(steamAppId: number): void {
  const existing = get(steamDetails)[steamAppId];
  // A failed attempt must not pin the title to an empty panel for the session.
  if (existing !== undefined && existing.status !== "error") return;
  void loadSteamDetails(steamAppId, "cache_or_fetch").catch(() => undefined);
}

export async function refreshSteamDetailsOnOpen(steamAppId: number, work: ArtworkWork): Promise<void> {
  if (!get(steamDetails)[steamAppId]?.details) await loadSteamDetails(steamAppId, "cache_or_fetch", work);
  await loadSteamDetails(steamAppId, "refresh_if_expired", work);
}
