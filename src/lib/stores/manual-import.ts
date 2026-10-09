import { t } from "../i18n";
import { get, writable } from "svelte/store";
import { pickExecutableFile, pickGameDirectory } from "../services/dialog";
import {
  identifyManualGameSteamAppId,
  importManualGame,
  scanGameExecutables,
  setGameExecutable,
  type ExecutableCandidate,
  type GameActionResult,
  type SteamIdentificationResult,
} from "../services/manual-import";
import type { Game } from "../services/local-state";
import { toMessage } from "../utils/errors";
import { reconcileGame, saveGame } from "./games";
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
let generation = 0;

export function resetManualImport(): void {
  generation++;
  manualImport.set(initial);
}

export function setScanGameName(gameName: string): void {
  manualImport.update((state) => ({ ...state, gameName }));
}

export function chooseCandidate(path: string): void {
  generation++;
  manualImport.update((state) => ({ ...state, selectedPath: path, status: "ready", error: null }));
}

export function clearSelection(): void {
  generation++;
  manualImport.update((state) => ({ ...state, selectedPath: null }));
}

export async function pickExecutable(startPath?: string | null): Promise<void> {
  const request = ++generation;
  try {
    const executable = await pickExecutableFile(startPath);
    if (request !== generation || executable === null) return;
    chooseCandidate(executable);
  } catch (error) {
    if (request !== generation) return;
    manualImport.update((state) => ({ ...state, error: toMessage(error) }));
  }
}

export async function rescanDirectory(directory: string): Promise<void> {
  const request = ++generation;
  const gameName = get(manualImport).gameName;
  manualImport.update((state) => ({
    ...state,
    directory,
    candidates: [], suggestedPath: null, selectedPath: null,
    status: "loading",
    error: null,
  }));
  try {
    const scan = await scanGameExecutables(directory, gameName.length > 0 ? gameName : null);
    if (request !== generation) return;
    manualImport.update((state) => ({
      ...state,
      status: scan.candidates.length === 0 ? "empty" : "ready",
      candidates: scan.candidates,
      suggestedPath: scan.selectedPath,
      selectedPath: scan.selectedPath,
    }));
  } catch (error) {
    if (request !== generation) return;
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
  const request = ++generation;
  try {
    const directory = await pickGameDirectory(startPath);
    if (request !== generation || directory === null) return;
    await rescanDirectory(directory);
  } catch (error) {
    if (request !== generation) return;
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
    throw new Error(t("Seleziona un eseguibile dalla scansione.", undefined));
  }
  return selected;
}

export interface ManualImportOutcome {
  game: Game;
  linkingError: string | null;
  shortcutWarning: string | null;
}

export async function importScannedGame(
  name: string | null,
  identity: { steamAppId: number; name: string } | null,
): Promise<ManualImportOutcome> {
  const { game, shortcutWarning } = await importManualGame({
    executablePath: selectedExecutable(),
    name: name !== null && name.trim().length > 0 ? name.trim() : null,
  });
  reconcileGame(game);
  if (identity === null) return { game, linkingError: null, shortcutWarning };
  try {
    const linked = await saveGame({
      id: game.id,
      steamAppId: identity.steamAppId,
      automaticName: identity.name,
      nameOverride: game.nameOverride,
    });
    return { game: linked, linkingError: null, shortcutWarning };
  } catch (error) {
    return { game, linkingError: toMessage(error), shortcutWarning };
  }
}

export async function detectManualGameSteamAppId(gameId: string): Promise<SteamIdentificationResult> {
  const result = await identifyManualGameSteamAppId(gameId);
  reconcileGame(result.game);
  return result;
}

export async function saveExecutableForGame(gameId: string, executablePath: string): Promise<GameActionResult> {
  const result = await setGameExecutable(gameId, executablePath);
  reconcileGame(result.game);
  return result;
}
