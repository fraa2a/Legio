import { invoke } from "@tauri-apps/api/core";

export type GraphicsRenderer = "runner_default" | "wine_d3d";
export type WaylandMode = "runner_default" | "disabled" | "native";

export function isGraphicsRenderer(value: string): value is GraphicsRenderer {
  return value === "runner_default" || value === "wine_d3d";
}

export function isWaylandMode(value: string): value is WaylandMode {
  return value === "runner_default" || value === "disabled" || value === "native";
}

export interface CompatibilityDefaults {
  runnerPath: string | null;
  prefixRoot: string | null;
  argumentsBefore: string[];
  argumentsAfter: string[];
  /** Legacy persisted field; global working directories are intentionally ignored. */
  workingDirectory: string | null;
  environment: Record<string, string>;
  dllOverrides: Record<string, string>;
  graphicsRenderer: GraphicsRenderer;
  wayland: WaylandMode;
  debugLogging: boolean;
}

export interface GameCompatibilityOverrides {
  launchViaSteam: boolean | null;
  runnerPath: string | null;
  prefixPath: string | null;
  argumentsBefore: string[] | null;
  argumentsAfter: string[] | null;
  workingDirectory: string | null;
  environment: Record<string, string> | null;
  dllOverrides: Record<string, string> | null;
  graphicsRenderer: GraphicsRenderer | null;
  wayland: WaylandMode | null;
  debugLogging: boolean | null;
  onlineFix: boolean | null;
}

export interface NativeLaunchConfig {
  arguments: string[];
  workingDirectory: string | null;
  environment: Record<string, string>;
}

export interface SteamLaunchConfig {
  arguments: string[];
}

export interface CompatibilityRunner {
  kind: "proton" | "ge_proton" | "wine";
  name: string;
  version: string;
  path: string;
}

export function compatibilityRunnerLabel(runner: CompatibilityRunner): string {
  return runner.kind === "wine" && runner.version ? `${runner.name} (${runner.version})` : runner.name;
}

export interface RunnerDiscovery {
  ntsyncAvailable: boolean;
  runners: CompatibilityRunner[];
  diagnostics: string[];
}

export const emptyCompatibilityDefaults: CompatibilityDefaults = {
  runnerPath: null,
  prefixRoot: null,
  argumentsBefore: [],
  argumentsAfter: [],
  workingDirectory: null,
  environment: {},
  dllOverrides: {},
  graphicsRenderer: "runner_default",
  wayland: "runner_default",
  debugLogging: false,
};

export const emptyGameCompatibilityOverrides: GameCompatibilityOverrides = {
  launchViaSteam: null,
  runnerPath: null,
  prefixPath: null,
  argumentsBefore: null,
  argumentsAfter: null,
  workingDirectory: null,
  environment: null,
  dllOverrides: null,
  graphicsRenderer: null,
  wayland: null,
  debugLogging: null,
  onlineFix: null,
};

export function getCompatibilityDefaults(): Promise<CompatibilityDefaults> {
  return invoke<CompatibilityDefaults>("get_compatibility_defaults");
}

export function saveCompatibilityDefaults(
  defaults: CompatibilityDefaults,
): Promise<CompatibilityDefaults> {
  return invoke<CompatibilityDefaults>("save_compatibility_defaults", { defaults });
}

export function getGameCompatibilityOverrides(gameId: string): Promise<GameCompatibilityOverrides> {
  return invoke<GameCompatibilityOverrides>("get_game_compatibility_overrides", { gameId });
}

export function getGameOnlineFixDetected(gameId: string): Promise<boolean> {
  return invoke<boolean>("get_game_online_fix_detected", { gameId });
}

export function saveGameCompatibilityOverrides(
  gameId: string,
  overrides: GameCompatibilityOverrides,
): Promise<GameCompatibilityOverrides> {
  return invoke<GameCompatibilityOverrides>("save_game_compatibility_overrides", {
    gameId,
    overrides,
  });
}

export function listCompatibilityRunners(): Promise<RunnerDiscovery> {
  return invoke<RunnerDiscovery>("list_compatibility_runners");
}

export function getNativeLaunchConfig(gameId: string): Promise<NativeLaunchConfig> {
  return invoke<NativeLaunchConfig>("get_native_launch_config", { gameId });
}

export function saveNativeLaunchConfig(
  gameId: string,
  config: NativeLaunchConfig,
): Promise<NativeLaunchConfig> {
  return invoke<NativeLaunchConfig>("save_native_launch_config", { gameId, config });
}

export function getSteamLaunchConfig(gameId: string): Promise<SteamLaunchConfig> {
  return invoke<SteamLaunchConfig>("get_steam_launch_config", { gameId });
}

export function saveSteamLaunchConfig(
  gameId: string,
  config: SteamLaunchConfig,
): Promise<SteamLaunchConfig> {
  return invoke<SteamLaunchConfig>("save_steam_launch_config", { gameId, config });
}

export function getCompatibilityLogsDirectory(): Promise<string> {
  return invoke<string>("get_compatibility_logs_directory");
}
