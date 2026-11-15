import { t } from "../../i18n";
import type { LinuxPerformance } from "../../services/game-settings";
import { mapToText, parseMap } from "../../services/launch-fields";
import type { LaunchOverrides, LaunchProfile, LaunchProfiles } from "../../services/launch-profiles";
import { formatLaunchArguments, parseLaunchArguments } from "../../utils/launch-arguments";

// Mirror MAX_PROFILE_NAME_CHARS and MAX_PROFILES_PER_GAME in src-tauri/src/database/launch_profiles.rs.
export const maxProfileNameLength = 48;
export const maxProfilesPerGame = 12;

export interface ProfileMenuOption {
  // The profile ID, or null for the base settings.
  id: string | null;
  name: string;
}

/** Form state of one profile: each override can be switched back to inheriting. */
export interface ProfileDraft {
  name: string;
  argumentsOverridden: boolean;
  argumentsText: string;
  environmentOverridden: boolean;
  environmentText: string;
  runnerOverridden: boolean;
  runnerPath: string;
  performanceOverridden: boolean;
  performance: LinuxPerformance;
}

export const emptyPerformance: LinuxPerformance = { gameMode: false, gamescope: null };

export function profilesOf(state: LaunchProfiles, gameId: string): LaunchProfile[] {
  return state.profiles.filter((profile) => profile.gameId === gameId);
}

/** The profile a game launched last, or null (the base settings) when it has none or it is gone. */
export function lastUsedProfileId(state: LaunchProfiles, gameId: string): string | null {
  const id = state.lastUsed[gameId];
  return id !== undefined && state.profiles.some((profile) => profile.id === id && profile.gameId === gameId) ? id : null;
}

export function profileMenuOptions(profiles: readonly LaunchProfile[], baseName: string): ProfileMenuOption[] {
  return [{ id: null, name: baseName }, ...profiles.map((profile) => ({ id: profile.id, name: profile.name }))];
}

export function profileName(profiles: readonly LaunchProfile[], id: string | null, baseName: string): string {
  return profiles.find((profile) => profile.id === id)?.name ?? baseName;
}

/** Returns a message when the name cannot be used, mirroring the backend rules. */
export function profileNameError(name: string, profiles: readonly LaunchProfile[], exceptId: string | null): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0 || [...trimmed].length > maxProfileNameLength) {
    return t("Il nome deve avere da 1 a {0} caratteri.", undefined, [String(maxProfileNameLength)]);
  }
  if (/\p{Cc}/u.test(trimmed)) return t("Il nome non può contenere caratteri di controllo.", undefined);
  const folded = trimmed.toLowerCase();
  if (profiles.some((profile) => profile.id !== exceptId && profile.name.toLowerCase() === folded)) {
    return t("Esiste già un profilo con questo nome.", undefined);
  }
  return null;
}

/** A free name for a copy, such as "Window (copia)" or "Window (copia 2)", within the length limit. */
export function copyName(name: string, profiles: readonly LaunchProfile[]): string {
  for (let index = 1; ; index++) {
    const format = (stem: string) => index === 1 ? t("{0} (copia)", undefined, [stem]) : t("{0} (copia {1})", undefined, [stem, String(index)]);
    const room = maxProfileNameLength - [...format("")].length;
    const candidate = format([...name].slice(0, room).join("").trimEnd());
    if (profileNameError(candidate, profiles, null) === null) return candidate;
  }
}

export function draftFromProfile(profile: LaunchProfile | null): ProfileDraft {
  const overrides = profile?.overrides;
  return {
    name: profile?.name ?? "",
    argumentsOverridden: overrides?.arguments != null,
    argumentsText: formatLaunchArguments(overrides?.arguments ?? []),
    environmentOverridden: overrides?.environment != null,
    environmentText: mapToText(overrides?.environment ?? {}),
    runnerOverridden: overrides?.runnerPath != null,
    runnerPath: overrides?.runnerPath ?? "",
    performanceOverridden: overrides?.linuxPerformance != null,
    performance: structuredClone(overrides?.linuxPerformance ?? emptyPerformance),
  };
}

/** Parses the form into overrides; a field left inheriting stays null. Throws a readable message on bad input. */
export function overridesFromDraft(draft: ProfileDraft): LaunchOverrides {
  if (draft.runnerOverridden && draft.runnerPath.length === 0) throw new Error(t("Scegli un runner per il profilo.", undefined));
  return {
    arguments: draft.argumentsOverridden ? parseLaunchArguments(draft.argumentsText) : null,
    environment: draft.environmentOverridden ? { ...parseMap(draft.environmentText, t("Variabili ambiente", undefined)) } : null,
    runnerPath: draft.runnerOverridden ? draft.runnerPath : null,
    linuxPerformance: draft.performanceOverridden ? structuredClone(draft.performance) : null,
  };
}

export function overrideCount(overrides: LaunchOverrides): number {
  return [overrides.arguments, overrides.environment, overrides.runnerPath, overrides.linuxPerformance].filter((value) => value !== null).length;
}
