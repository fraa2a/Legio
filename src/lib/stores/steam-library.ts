import { writable } from "svelte/store";
import {
  importSteamInstallations,
  scanSteamInstallations,
  type SteamImportResult,
  type SteamScan,
} from "../services/steam";
import { toMessage } from "../utils/errors";
import { games } from "./games";
import type { LoadStatus } from "./resource";

interface SteamLibraryState {
  status: LoadStatus;
  scan: SteamScan | null;
  importResult: SteamImportResult | null;
  importing: boolean;
  error: string | null;
}

const initial: SteamLibraryState = {
  status: "idle",
  scan: null,
  importResult: null,
  importing: false,
  error: null,
};

export const steamLibrary = writable<SteamLibraryState>(initial);

export function resetSteamLibrary(): void {
  steamLibrary.set(initial);
}

export async function scanSteamLibrary(): Promise<void> {
  steamLibrary.update((state) => ({ ...state, status: "loading", error: null }));
  try {
    const scan = await scanSteamInstallations();
    steamLibrary.set({ ...initial, status: "ready", scan });
  } catch (error) {
    steamLibrary.update((state) => ({
      ...state,
      status: "error",
      error: toMessage(error),
    }));
  }
}

export async function importSteamLibrary(): Promise<void> {
  steamLibrary.update((state) => ({ ...state, importing: true, error: null }));
  try {
    const importResult = await importSteamInstallations();
    await games.load();
    steamLibrary.update((state) => ({ ...state, importing: false, importResult }));
  } catch (error) {
    steamLibrary.update((state) => ({
      ...state,
      importing: false,
      error: toMessage(error),
    }));
  }
}
