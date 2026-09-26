import { get, writable } from "svelte/store";
import {
  getSteamDetails,
  type SteamDetails,
  type SteamDetailsResult,
} from "../services/steam-details";
import { toMessage } from "../utils/errors";
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

const inflight = new Map<number, Promise<SteamDetailsResult>>();

function patch(steamAppId: number, values: Partial<SteamDetailsState>): void {
  steamDetails.update((all) => ({ ...all, [steamAppId]: { ...(all[steamAppId] ?? idle), ...values } }));
}

export function loadSteamDetails(
  steamAppId: number,
  refresh: boolean,
): Promise<SteamDetailsResult> {
  if (!refresh) {
    const pending = inflight.get(steamAppId);
    if (pending !== undefined) return pending;
  }
  const request = (async () => {
    patch(steamAppId, { status: "loading", error: null });
    try {
      const result = await getSteamDetails(steamAppId, refresh);
      patch(steamAppId, {
        status: result.details === null ? "empty" : "ready",
        details: result.details,
        cachedAt: result.cachedAt,
        stale: result.stale,
        error: null,
      });
      return result;
    } catch (error) {
      patch(steamAppId, { status: "error", error: toMessage(error) });
      throw error;
    } finally {
      inflight.delete(steamAppId);
    }
  })();
  if (!refresh) inflight.set(steamAppId, request);
  return request;
}

export function ensureSteamDetails(steamAppId: number): void {
  if (get(steamDetails)[steamAppId] !== undefined) return;
  void loadSteamDetails(steamAppId, false).catch(() => undefined);
}
