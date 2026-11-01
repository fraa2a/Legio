import { derived, get, readable, writable, type Readable } from "svelte/store";
import {
  bindGameToEcosystem,
  cancelModInstall,
  createModProfile,
  deleteModProfile,
  duplicateModProfile,
  getModLaunchStatus,
  getModPackage,
  importLocalMod,
  installModLoader,
  installMods,
  listEcosystemGames,
  listModdableGames,
  listModProfiles,
  listModUpdates,
  listProfileMods,
  observeCatalogProgress,
  observeInstallProgress,
  refreshModCatalog,
  renameModProfile,
  restoreModdedGameFolder,
  setActiveModProfile,
  setProfileModEnabled,
  uninstallProfileMod,
  updateProfileMods,
  type CatalogProgress,
  type CatalogStatus,
  type DeleteProfileOutcome,
  type EcosystemGameSummary,
  type GameModding,
  type InstallProgress,
  type LocalImportOutcome,
  type ModdableGame,
  type ModInstallRequest,
  type ModInstallResult,
  type ModLaunchStatus,
  type ModProfile,
  type ModUpdate,
  type PackageDetails,
  type ProfileMod,
  type UninstallOutcome,
} from "../services/modding";
import { toMessage } from "../utils/errors";
import { createResource } from "./resource";

type Resource<T> = ReturnType<typeof createResource<T>>;

export const moddableGames = createResource<ModdableGame[]>([], listModdableGames, (games) => games.length === 0);

export const moddableGameById = derived(moddableGames, (state) => new Map(state.data.map((game) => [game.gameId, game])));

// The list needs the Thunderstore game list, so it is read once and then kept in sync by the actions below.
export function loadModdableGames(): Promise<void> {
  const { status } = get(moddableGames);
  return status === "idle" || status === "error" ? moddableGames.load() : Promise.resolve();
}

export const ecosystemGames = createResource<EcosystemGameSummary[]>([], listEcosystemGames, (games) => games.length === 0);

function applyBinding(binding: GameModding): void {
  const { data } = get(moddableGames);
  moddableGames.set(data.map((game) => game.gameId === binding.gameId ? { ...game, binding } : game));
}

export async function bindGame(gameId: string, ecosystemGameId: string): Promise<void> {
  applyBinding(await bindGameToEcosystem(gameId, ecosystemGameId));
  await moddableGames.reload();
}

function cached<T>(resources: Map<string, Resource<T>>, key: string, create: () => Resource<T>): Resource<T> {
  let resource = resources.get(key);
  if (resource === undefined) {
    resource = create();
    resources.set(key, resource);
  }
  return resource;
}

const profileResources = new Map<string, Resource<ModProfile[]>>();
const modResources = new Map<string, Resource<ProfileMod[]>>();
const updateResources = new Map<string, Resource<ModUpdate[]>>();
const launchResources = new Map<string, Resource<ModLaunchStatus | null>>();

export function modProfiles(gameId: string): Resource<ModProfile[]> {
  return cached(profileResources, gameId, () =>
    createResource<ModProfile[]>([], () => listModProfiles(gameId), (profiles) => profiles.length === 0));
}

export function profileMods(profileId: string): Resource<ProfileMod[]> {
  return cached(modResources, profileId, () =>
    createResource<ProfileMod[]>([], () => listProfileMods(profileId), (mods) => mods.length === 0));
}

export function modUpdates(profileId: string): Resource<ModUpdate[]> {
  return cached(updateResources, profileId, () => createResource<ModUpdate[]>([], () => listModUpdates(profileId)));
}

export function modLaunchStatus(gameId: string): Resource<ModLaunchStatus | null> {
  return cached(launchResources, gameId, () => createResource<ModLaunchStatus | null>(null, () => getModLaunchStatus(gameId)));
}

export function loadOnce<T>(resource: Resource<T>): void {
  if (get(resource).status === "idle") void resource.load();
}

export async function createProfile(gameId: string, name: string): Promise<ModProfile> {
  const profile = await createModProfile(gameId, name.trim());
  await modProfiles(gameId).reload();
  return profile;
}

export async function renameProfile(gameId: string, profileId: string, name: string): Promise<void> {
  await renameModProfile(profileId, name.trim());
  await modProfiles(gameId).reload();
}

export async function duplicateProfile(gameId: string, profileId: string, name: string): Promise<ModProfile> {
  const profile = await duplicateModProfile(profileId, name.trim());
  await modProfiles(gameId).reload();
  return profile;
}

export async function deleteProfile(gameId: string, profileId: string): Promise<DeleteProfileOutcome> {
  const outcome = await deleteModProfile(profileId);
  modResources.delete(profileId);
  updateResources.delete(profileId);
  const binding = get(moddableGameById).get(gameId)?.binding ?? null;
  if (binding?.activeProfileId === profileId) applyBinding({ ...binding, activeProfileId: null });
  await Promise.all([modProfiles(gameId).reload(), modLaunchStatus(gameId).reload()]);
  return outcome;
}

export async function selectProfile(gameId: string, profileId: string | null): Promise<void> {
  applyBinding(await setActiveModProfile(gameId, profileId));
  await modLaunchStatus(gameId).reload();
}

export async function restoreGameFolder(gameId: string): Promise<void> {
  await restoreModdedGameFolder(gameId);
  await modLaunchStatus(gameId).reload();
}

// Profiles with a running install, update or import, so their controls can wait for it.
export const busyProfiles = writable<ReadonlySet<string>>(new Set());

const progressState = writable<ReadonlyMap<string, InstallProgress>>(new Map(), (_set, update) =>
  listenWhileObserved(observeInstallProgress, (progress) => update((current) => {
    const next = new Map(current);
    if (progress.stage === "done") next.delete(progress.profileId);
    else next.set(progress.profileId, progress);
    return next;
  })));

export const installProgress: Readable<ReadonlyMap<string, InstallProgress>> = { subscribe: progressState.subscribe };

function listenWhileObserved<T>(
  observe: (callback: (payload: T) => void) => Promise<() => void>,
  callback: (payload: T) => void,
): () => void {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  observe(callback).then((stop) => {
    if (disposed) stop();
    else unlisten = stop;
  }).catch((error: unknown) => console.error(error));
  return () => {
    disposed = true;
    unlisten?.();
  };
}

async function refreshProfile(gameId: string, profileId: string): Promise<void> {
  await Promise.all([profileMods(profileId).reload(), modUpdates(profileId).reload(), modLaunchStatus(gameId).reload()]);
}

async function track<T>(gameId: string, profileId: string, operation: () => Promise<T>): Promise<T> {
  busyProfiles.update((current) => new Set(current).add(profileId));
  try {
    return await operation();
  } finally {
    busyProfiles.update((current) => {
      const next = new Set(current);
      next.delete(profileId);
      return next;
    });
    progressState.update((current) => {
      if (!current.has(profileId)) return current;
      const next = new Map(current);
      next.delete(profileId);
      return next;
    });
    await refreshProfile(gameId, profileId).catch((error: unknown) => console.error(error));
  }
}

export function installPackages(gameId: string, profileId: string, packages: ModInstallRequest[]): Promise<ModInstallResult> {
  return track(gameId, profileId, () => installMods(profileId, packages));
}

export function updateMods(gameId: string, profileId: string, fullNames: string[] | null = null): Promise<ModInstallResult> {
  return track(gameId, profileId, () => updateProfileMods(profileId, fullNames));
}

export function importLocalArchive(gameId: string, profileId: string, archivePath: string): Promise<LocalImportOutcome> {
  return track(gameId, profileId, () => importLocalMod(profileId, archivePath));
}

export function setModEnabled(gameId: string, profileId: string, fullName: string, enabled: boolean): Promise<void> {
  return track(gameId, profileId, () => setProfileModEnabled(profileId, fullName, enabled));
}

export async function uninstallMod(gameId: string, profileId: string, fullName: string): Promise<UninstallOutcome> {
  const outcome = await uninstallProfileMod(profileId, fullName);
  await refreshProfile(gameId, profileId);
  return outcome;
}

export function installLoader(gameId: string, profileId: string): Promise<ModInstallResult> {
  return track(gameId, profileId, () => installModLoader(profileId));
}

export const cancelInstall = (profileId: string) => cancelModInstall(profileId);

export interface CatalogState {
  status: CatalogStatus | null;
  refreshing: boolean;
  error: string | null;
  progress: CatalogProgress | null;
}

const idleCatalog: CatalogState = { status: null, refreshing: false, error: null, progress: null };

const catalogState = writable<Readonly<Record<string, CatalogState>>>({}, (_set, update) =>
  listenWhileObserved(observeCatalogProgress, (progress) => update((current) => {
    const entry = current[progress.community];
    return entry?.refreshing ? { ...current, [progress.community]: { ...entry, progress } } : current;
  })));

export const catalogs: Readable<Readonly<Record<string, CatalogState>>> = { subscribe: catalogState.subscribe };

export function catalogFor(state: Readonly<Record<string, CatalogState>>, ecosystemGameId: string): CatalogState {
  return state[ecosystemGameId] ?? idleCatalog;
}

const packageCache = new Map<string, Promise<PackageDetails | null>>();
const packageCacheLimit = 200;

function patchCatalog(ecosystemGameId: string, changes: Partial<CatalogState>): void {
  catalogState.update((current) => ({ ...current, [ecosystemGameId]: { ...catalogFor(current, ecosystemGameId), ...changes } }));
}

export async function refreshCatalog(ecosystemGameId: string, force: boolean): Promise<void> {
  if (catalogFor(get(catalogState), ecosystemGameId).refreshing) return;
  patchCatalog(ecosystemGameId, { refreshing: true, error: null, progress: null });
  try {
    const status = await refreshModCatalog(ecosystemGameId, force);
    const previous = catalogFor(get(catalogState), ecosystemGameId).status;
    if (previous?.syncedAt !== status.syncedAt) {
      for (const key of packageCache.keys()) if (key.startsWith(`${ecosystemGameId}:`)) packageCache.delete(key);
    }
    patchCatalog(ecosystemGameId, { status, refreshing: false, progress: null });
  } catch (error) {
    patchCatalog(ecosystemGameId, { refreshing: false, error: toMessage(error), progress: null });
  }
}

// The backend skips the download within an hour of the last check, so this is cheap to call on open.
export function ensureCatalog(ecosystemGameId: string): Promise<void> {
  const state = catalogFor(get(catalogState), ecosystemGameId);
  return state.status === null && !state.refreshing ? refreshCatalog(ecosystemGameId, false) : Promise.resolve();
}

export function packageDetails(ecosystemGameId: string, fullName: string): Promise<PackageDetails | null> {
  const key = `${ecosystemGameId}:${fullName.toLowerCase()}`;
  let pending = packageCache.get(key);
  if (pending === undefined) {
    pending = getModPackage(ecosystemGameId, fullName);
    pending.catch(() => packageCache.delete(key));
    packageCache.set(key, pending);
    // Maps keep insertion order, so the first key is the oldest request.
    const oldest = packageCache.keys().next();
    if (packageCache.size > packageCacheLimit && oldest.done !== true) packageCache.delete(oldest.value);
  }
  return pending;
}

const noProfiles: Readable<ReadonlyMap<string, readonly ModProfile[]>> = readable(new Map());

// Profiles of several instances at once, for pickers that list them side by side.
export function profilesOfGames(gameIds: readonly string[]): Readable<ReadonlyMap<string, readonly ModProfile[]>> {
  if (gameIds.length === 0) return noProfiles;
  return derived(gameIds.map(modProfiles), (states) => new Map(gameIds.map((gameId, index) => [gameId, states[index].data])));
}
