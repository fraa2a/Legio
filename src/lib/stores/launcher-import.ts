import { get, writable } from "svelte/store";
import { importLauncherGames, scanLauncherGames, type DetectedGame, type ImportItem, type Launcher, type LauncherReport } from "../services/launcher-import";
import { toMessage } from "../utils/errors";
import { games } from "./games";

export interface LauncherGroup { launcher: Launcher; games: DetectedGame[] }
interface State {
  groups: LauncherGroup[];
  selected: string[];
  results: ImportItem[];
  importing: boolean;
  error: string | null;
}

const initial = (): State => ({ groups: [], selected: [], results: [], importing: false, error: null });
export const launcherImport = writable<State>(initial());
let scanId = 0;

/** Only launchers with at least one detected game are offered. */
export function groupsFromReports(reports: LauncherReport[]): LauncherGroup[] {
  return reports.flatMap((report) => report.status === "found" && report.games.length > 0 ? [{ launcher: report.launcher, games: report.games }] : []);
}

export const isImportable = (game: DetectedGame): boolean => game.executable !== null;
const importableKeys = (group: LauncherGroup): string[] => group.games.filter(isImportable).map((game) => game.key);

export function launcherSelection(group: LauncherGroup, selected: string[]): { count: number; total: number; all: boolean } {
  const keys = importableKeys(group);
  const count = keys.filter((key) => selected.includes(key)).length;
  return { count, total: keys.length, all: keys.length > 0 && count === keys.length };
}

export async function scanLaunchers(): Promise<void> {
  const request = ++scanId;
  try {
    const groups = groupsFromReports(await scanLauncherGames());
    if (request === scanId) launcherImport.set({ ...initial(), groups });
  } catch (error) {
    // Detection is optional: the step stays hidden and the failure is only logged.
    console.error("Could not scan other launchers", toMessage(error));
  }
}

export function resetLauncherImport(): void {
  ++scanId;
  launcherImport.set(initial());
}

export function toggleGame(key: string): void {
  launcherImport.update((state) => state.importing ? state : { ...state, selected: state.selected.includes(key) ? state.selected.filter((item) => item !== key) : [...state.selected, key] });
}

export function toggleLauncher(launcher: Launcher): void {
  launcherImport.update((state) => {
    const group = state.groups.find((item) => item.launcher === launcher);
    if (!group || state.importing) return state;
    const keys = importableKeys(group);
    const rest = state.selected.filter((key) => !keys.includes(key));
    return { ...state, selected: launcherSelection(group, state.selected).all ? rest : [...rest, ...keys] };
  });
}

/** Selected games already imported or skipped are not offered again. */
export async function runLauncherImport(): Promise<void> {
  const state = get(launcherImport);
  if (state.importing || state.selected.length === 0) return;
  const selection = state.groups.flatMap((group) => group.games.filter((game) => state.selected.includes(game.key)).map((game) => ({ launcher: game.launcher, key: game.key })));
  launcherImport.update((current) => ({ ...current, importing: true, error: null }));
  try {
    const results = await importLauncherGames(selection);
    if (results.some((item) => item.status === "imported")) await games.reload();
    const done = new Set(results.filter((item) => item.status !== "failed").map((item) => item.key));
    launcherImport.update((current) => ({ ...current, results: [...current.results.filter((item) => !results.some((fresh) => fresh.key === item.key)), ...results], selected: current.selected.filter((key) => !done.has(key)) }));
  } catch (error) {
    launcherImport.update((current) => ({ ...current, error: toMessage(error) }));
  } finally {
    launcherImport.update((current) => ({ ...current, importing: false }));
  }
}
