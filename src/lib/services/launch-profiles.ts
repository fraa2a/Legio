import { invoke } from "./invoke";
import type { LinuxPerformance } from "./game-settings";

/** Settings a profile replaces; null inherits the game's base settings. */
export interface LaunchOverrides {
  arguments: string[] | null;
  environment: Record<string, string> | null;
  runnerPath: string | null;
  linuxPerformance: LinuxPerformance | null;
}

export interface LaunchProfile {
  id: string;
  gameId: string;
  name: string;
  overrides: LaunchOverrides;
}

export interface LaunchProfiles {
  profiles: LaunchProfile[];
  /** Game ID to the profile it launched last; a game without an entry uses its base settings. */
  lastUsed: Record<string, string>;
}

export function getLaunchProfiles(): Promise<LaunchProfiles> {
  return invoke<LaunchProfiles>("get_launch_profiles");
}

export function createLaunchProfile(gameId: string, name: string, overrides: LaunchOverrides): Promise<LaunchProfiles> {
  return invoke<LaunchProfiles>("create_launch_profile", { gameId, name, overrides });
}

export function updateLaunchProfile(id: string, name: string, overrides: LaunchOverrides): Promise<LaunchProfiles> {
  return invoke<LaunchProfiles>("update_launch_profile", { id, name, overrides });
}

export function duplicateLaunchProfile(id: string, name: string): Promise<LaunchProfiles> {
  return invoke<LaunchProfiles>("duplicate_launch_profile", { id, name });
}

export function deleteLaunchProfile(id: string): Promise<LaunchProfiles> {
  return invoke<LaunchProfiles>("delete_launch_profile", { id });
}
