import { get, writable } from "svelte/store";
import { pickExecutableFile, pickGameDirectory } from "../services/dialog";
import {
  importManualGame,
  scanGameExecutables,
  setGameExecutable,
  type ExecutableCandidate,
} from "../services/manual-import";
import type { Game } from "../services/local-state";
import { toMessage } from "../utils/errors";
import { reconcileGame } from "./games";
import type { LoadStatus } from "./resource";

interface ManualImportState {
  directory: string | null;
  gameName: string;
  status: LoadStatus;
  candidates: ExecutableCandidate[];
  suggestedPath: string | null;
  selectedPath: string | null;
  error: string | null;
}

const initial: ManualImportState = {
  directory: null,
  gameName: "",
  status: "idle",
  candidates: [],
  suggestedPath: null,
  selectedPath: null,
  error: null,
};

export const manualImport = writable<ManualImportState>(initial);

export function resetManualImport(): void {
  manualImport.set(initial);
}

export function setScanGameName(gameName: string): void {
  manualImport.update((state) => ({ ...state, gameName }));
}

export function chooseCandidate(path: string): void {
  manualImport.update((state) => ({ ...state, selectedPath: path }));
}

export function clearSelection(): void {
  manualImport.update((state) => ({ ...state, selectedPath: null }));
}

export async function pickExecutable(startPath?: string | null): Promise<void> {
  try {
    const executable = await pickExecutableFile(startPath);
    if (executable === null) return;
    chooseCandidate(executable);
  } catch (error) {
    manualImport.update((state) => ({ ...state, error: toMessage(error) }));
  }
}

export async function rescanDirectory(directory: string): Promise<void> {
  const gameName = get(manualImport).gameName;
  manualImport.update((state) => ({
    ...state,
    directory,
    status: "loading",
    error: null,
  }));
  try {
    const scan = await scanGameExecutables(directory, gameName.length > 0 ? gameName : null);
    manualImport.update((state) => ({
      ...state,
      status: scan.candidates.length === 0 ? "empty" : "ready",
      candidates: scan.candidates,
      suggestedPath: scan.selectedPath,
      selectedPath: scan.selectedPath,
    }));
  } catch (error) {
    manualImport.update((state) => ({
      ...state,
      status: "error",
      candidates: [],
      suggestedPath: null,
      selectedPath: null,
      error: toMessage(error),
    }));
  }
}

export async function browseGameDirectory(startPath?: string | null): Promise<void> {
  try {
    const directory = await pickGameDirectory(startPath);
    if (directory === null) return;
    await rescanDirectory(directory);
  } catch (error) {
    manualImport.update((state) => ({ ...state, error: toMessage(error) }));
  }
}

export async function rescanCurrentDirectory(): Promise<void> {
  const directory = get(manualImport).directory;
  if (directory === null) return;
  await rescanDirectory(directory);
}

function selectedExecutable(): string {
  const selected = get(manualImport).selectedPath;
  if (selected === null) {
    throw new Error("Seleziona un eseguibile dalla scansione.");
  }
  return selected;
}

export async function importScannedGame(name: string | null): Promise<Game> {
  const game = await importManualGame({
    executablePath: selectedExecutable(),
    name: name !== null && name.trim().length > 0 ? name.trim() : null,
  });
  reconcileGame(game);
  return game;
}

export async function saveExecutableForGame(gameId: string): Promise<Game> {
  const game = await setGameExecutable(gameId, selectedExecutable());
  reconcileGame(game);
  return game;
}
