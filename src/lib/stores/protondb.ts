import { get, writable } from "svelte/store";
import { getProtonDbSummary, type ProtonDbSummary } from "../services/protondb";
import { appInfo } from "./app-info";
import type { LoadStatus } from "./resource";

export interface ProtonDbState {
  status: LoadStatus;
  summary: ProtonDbSummary | null;
}

export const protonDb = writable<Record<number, ProtonDbState>>({});

const inflight = new Map<number, Promise<void>>();

function patch(steamAppId: number, state: ProtonDbState): void {
  protonDb.update((all) => ({ ...all, [steamAppId]: state }));
}

// Linux only. A failure leaves no rating: the backend logs it and the page shows nothing.
export function ensureProtonDb(steamAppId: number): Promise<void> {
  if (get(appInfo).data.platform !== "linux") return Promise.resolve();
  const status = get(protonDb)[steamAppId]?.status;
  if (status === "ready" || status === "empty") return Promise.resolve();
  const pending = inflight.get(steamAppId);
  if (pending !== undefined) return pending;
  patch(steamAppId, { status: "loading", summary: null });
  const request = getProtonDbSummary(steamAppId)
    .then(
      ({ summary }) => patch(steamAppId, { status: summary === null ? "empty" : "ready", summary }),
      () => patch(steamAppId, { status: "error", summary: null }),
    )
    .finally(() => inflight.delete(steamAppId));
  inflight.set(steamAppId, request);
  return request;
}
