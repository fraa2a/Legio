import { invoke } from "./invoke";

export type Launcher = "heroic" | "lutris" | "itch" | "bottles";

export interface DetectedGame {
  key: string;
  launcher: Launcher;
  title: string;
  installDir: string;
  executable: string | null;
  kind: "native" | "windows";
  prefix: string | null;
  runnerHint: string | null;
  storeId: string | null;
}

export type LauncherReport = { launcher: Launcher } & (
  | { status: "notInstalled" }
  | { status: "unreadable"; reason: string }
  | { status: "found"; games: DetectedGame[]; warnings: string[] }
);

export interface ImportSelection {
  launcher: Launcher;
  key: string;
}

export type ImportItem = ImportSelection & { title: string } & (
  | { status: "imported"; warning: string | null }
  | { status: "duplicate" }
  | { status: "failed"; reason: string }
);

export const scanLauncherGames = () => invoke<LauncherReport[]>("scan_launcher_games");
export const importLauncherGames = (selection: ImportSelection[]) => invoke<ImportItem[]>("import_launcher_games", { selection });
