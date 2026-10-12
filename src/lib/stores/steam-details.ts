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
interface MetadataTask {
  promise: Promise<SteamDetailsResult>;
  work: ArtworkWork;
  consumers: Set<ArtworkWork>;
}
const inflight = new Map<number, MetadataTask>();

language.subscribe(() => {
  for (const [id, generation] of generations) generations.set(id, generation + 1);
  for (const task of inflight.values()) task.work.release();
  inflight.clear();
  steamDetails.set({});
});

function patch(steamAppId: number, values: Partial<SteamDetailsState>): void {
  steamDetails.update((all) => ({ ...all, [steamAppId]: { ...(all[steamAppId] ?? idle), ...values } }));
}

function retainMetadata(task: MetadataTask, consumer: ArtworkWork, owned: boolean): Promise<SteamDetailsResult> {
  task.consumers.add(consumer);
  const release = () => {
    task.consumers.delete(consumer);
    if (task.consumers.size === 0) task.work.release();
  };
  consumer.signal?.addEventListener("abort", release, { once: true });
  if (consumer.signal?.aborted) release();
  return task.promise.finally(() => {
    consumer.signal?.removeEventListener("abort", release);
    release();
    if (owned) consumer.release();
  });
}

export function loadSteamDetails(
  steamAppId: number,
  policy: DetailsPolicy | boolean,
  suppliedWork?: ArtworkWork,
): Promise<SteamDetailsResult> {
  const actualPolicy = typeof policy === "boolean" ? (policy ? "force_refresh" : "cache_or_fetch") : policy;
  const consumer = suppliedWork ?? createArtworkWork(steamAppId, "visible");
  if (actualPolicy !== "force_refresh") {
    const pending = inflight.get(steamAppId);
    if (pending !== undefined && !pending.work.signal?.aborted) return retainMetadata(pending, consumer, !suppliedWork);
  }
  const generation = (generations.get(steamAppId) ?? 0) + 1;
  generations.set(steamAppId, generation);
  const work = createArtworkWork(steamAppId, consumer.origin);
  const task: MetadataTask = { promise: Promise.resolve({ details: null, cachedAt: null, stale: false }), work, consumers: new Set() };
  const abandoned = () => {
    if (generations.get(steamAppId) === generation) {
      const previous = get(steamDetails)[steamAppId];
      patch(steamAppId, { status: previous?.details ? "ready" : "idle", error: null });
    }
  };
  work.signal?.addEventListener("abort", abandoned, { once: true });
  task.promise = (async () => {
    const previous = get(steamDetails)[steamAppId];
    patch(steamAppId, { status: previous?.details ? "ready" : "loading", error: null });
    try {
      const result = await getSteamDetails(steamAppId, actualPolicy, work);
      if (generations.get(steamAppId) === generation && !work.signal?.aborted) patch(steamAppId, {
        status: result.details === null ? "empty" : "ready",
        details: result.details,
        cachedAt: result.cachedAt,
        stale: result.stale,
        error: null,
      });
      return result;
    } catch (error) {
      if (generations.get(steamAppId) === generation && !work.signal?.aborted && !isArtworkCancelled(error)) {
        const previous = get(steamDetails)[steamAppId];
        patch(steamAppId, { status: previous?.details ? "ready" : "error", stale: true, error: toMessage(error) });
      }
      throw error;
    } finally {
      work.signal?.removeEventListener("abort", abandoned);
      work.release();
      if (generations.get(steamAppId) === generation) inflight.delete(steamAppId);
    }
  })();
  inflight.set(steamAppId, task);
  return retainMetadata(task, consumer, !suppliedWork);
}

export function ensureSteamDetails(steamAppId: number): () => void {
  const existing = get(steamDetails)[steamAppId];
  if (existing !== undefined && (existing.status === "ready" || existing.status === "empty")) return () => {};
  const work = createArtworkWork(steamAppId, "visible");
  void loadSteamDetails(steamAppId, "cache_or_fetch", work).catch(() => undefined).finally(() => work.release());
  return () => work.release();
}

export async function refreshSteamDetailsOnOpen(steamAppId: number, work: ArtworkWork): Promise<void> {
  if (!get(steamDetails)[steamAppId]?.details) await loadSteamDetails(steamAppId, "cache_or_fetch", work);
  await loadSteamDetails(steamAppId, "refresh_if_expired", work);
}
