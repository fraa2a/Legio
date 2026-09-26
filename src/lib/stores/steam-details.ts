import { writable } from "svelte/store";
import { getSteamDetails, type SteamDetails } from "../services/steam-details";
import { toMessage } from "../utils/errors";
import type { LoadStatus } from "./resource";

interface SteamDetailsState {
  appId: number | null;
  status: LoadStatus;
  details: SteamDetails | null;
  cachedAt: number | null;
  stale: boolean;
  error: string | null;
}

const initial: SteamDetailsState = {
  appId: null,
  status: "idle",
  details: null,
  cachedAt: null,
  stale: false,
  error: null,
};

export const steamDetails = writable<SteamDetailsState>(initial);

let requestId = 0;

export async function loadSteamDetails(appId: number, refresh: boolean): Promise<void> {
  const request = ++requestId;
  steamDetails.update((state) => ({ ...state, appId, status: "loading", error: null }));
  try {
    const result = await getSteamDetails(appId, refresh);
    if (request !== requestId) return;
    steamDetails.set({
      appId,
      status: result.details === null ? "empty" : "ready",
      details: result.details,
      cachedAt: result.cachedAt,
      stale: result.stale,
      error: null,
    });
  } catch (error) {
    if (request !== requestId) return;
    steamDetails.update((state) => ({
      ...state,
      status: "error",
      error: toMessage(error),
    }));
  }
}

export function clearSteamDetails(): void {
  requestId += 1;
  steamDetails.set(initial);
}
