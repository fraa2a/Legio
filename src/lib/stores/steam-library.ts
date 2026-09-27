import { writable } from "svelte/store";
import {
  importSteamInstallations,
  type SteamImportResult,
} from "../services/steam";
import { toMessage } from "../utils/errors";
import { games } from "./games";
import type { LoadStatus } from "./resource";

interface SteamLibraryState {
  status: LoadStatus;
  importResult: SteamImportResult | null;
  importing: boolean;
  error: string | null;
}

const initial: SteamLibraryState = {
  status: "idle",
  importResult: null,
  importing: false,
  error: null,
};

export const steamLibrary = writable<SteamLibraryState>(initial);

let importRequest: Promise<void> | null = null;

export function importSteamLibrary(): Promise<void> {
  if (importRequest !== null) return importRequest;
  importRequest = syncSteamLibrary();
  return importRequest;
}

async function syncSteamLibrary(): Promise<void> {
  steamLibrary.update((state) => ({ ...state, status: "loading", importing: true, error: null }));
  try {
    const importResult = await importSteamInstallations();
    await games.load();
    steamLibrary.set({ status: "ready", importing: false, importResult, error: null });
  } catch (error) {
    steamLibrary.update((state) => ({
      ...state,
      status: "error",
      importing: false,
      error: toMessage(error),
    }));
  } finally {
    importRequest = null;
  }
}
