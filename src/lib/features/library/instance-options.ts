import type { Game } from "../../services/local-state";
import type { ModdableGame, ModProfile } from "../../services/modding";
import type { ReleaseOption } from "./ReleasePicker.svelte";

// Game and profile ids are UUIDs, so a colon never appears inside one.
const separator = ":";

export interface InstanceTarget {
  gameId: string;
  profileId: string | null;
}

// What the picker needs to know about the mod profiles of one installed instance.
export interface InstanceModState {
  profiles: readonly ModProfile[];
  activeProfileId: string | null;
  // The instance is running or its active profile is being changed: the profile cannot be switched now.
  locked: boolean;
}

export function instanceOptionId(gameId: string, profileId: string | null = null): string {
  return profileId === null ? gameId : `${gameId}${separator}${profileId}`;
}

export function parseInstanceOptionId(id: string): InstanceTarget {
  const index = id.indexOf(separator);
  return index < 0 ? { gameId: id, profileId: null } : { gameId: id.slice(0, index), profileId: id.slice(index + 1) };
}

export function hasModProfiles(game: ModdableGame | undefined): boolean {
  return game !== undefined && game.binding !== null && game.ecosystemGameId !== null && game.supported;
}

export function instanceModStates(
  gameIds: readonly string[],
  games: ReadonlyMap<string, ModdableGame>,
  profiles: ReadonlyMap<string, readonly ModProfile[]>,
  isLocked: (gameId: string, activeProfileId: string | null) => boolean,
): Map<string, InstanceModState> {
  const states = new Map<string, InstanceModState>();
  for (const gameId of gameIds) {
    const game = games.get(gameId);
    if (!hasModProfiles(game)) continue;
    const activeProfileId = game?.binding?.activeProfileId ?? null;
    states.set(gameId, { profiles: profiles.get(gameId) ?? [], activeProfileId, locked: isLocked(gameId, activeProfileId) });
  }
  return states;
}

export function selectedInstanceId(gameId: string, state: InstanceModState | undefined): string {
  const active = state?.profiles.find((profile) => profile.id === state.activeProfileId);
  return instanceOptionId(gameId, active?.id ?? null);
}

export function changesProfile(target: InstanceTarget, state: InstanceModState | undefined): boolean {
  return state !== undefined && state.activeProfileId !== target.profileId;
}

// Each instance is followed by one entry per mod profile, all based on that instance.
export function instanceReleaseOptions(bases: readonly ReleaseOption[], states: ReadonlyMap<string, InstanceModState>): ReleaseOption[] {
  return bases.flatMap((base) => {
    const state = states.get(base.id);
    if (state === undefined) return [base];
    const blocked = (profileId: string | null) => state.locked && state.activeProfileId !== profileId;
    const profiles = [...state.profiles]
      .sort((left, right) => left.name.localeCompare(right.name, undefined, { sensitivity: "base" }))
      .map((profile): ReleaseOption => ({
        id: instanceOptionId(base.id, profile.id),
        name: base.name,
        subtitle: base.subtitle,
        profileName: profile.name,
        disabled: blocked(profile.id),
      }));
    return [{ ...base, disabled: blocked(null) }, ...profiles];
  });
}

// Last two folders of the install, enough to tell apart copies of one game kept in different places.
export function instanceLocation(game: Game | undefined): string | null {
  if (game === undefined) return null;
  const folder = game.steamInstallPath ?? game.installationRoot;
  const segments = (folder ?? game.executablePath?.replace(/[\\/][^\\/]*$/, "") ?? "").split(/[\\/]/).filter((segment) => segment.length > 0);
  return segments.length === 0 ? null : segments.slice(-2).join("/");
}

export function hasDuplicateName(game: ModdableGame, games: readonly ModdableGame[]): boolean {
  return games.some((other) => other.gameId !== game.gameId && other.name === game.name);
}
