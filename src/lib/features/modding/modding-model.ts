import { t, type Language } from "../../i18n";
import type {
  EcosystemGameSummary,
  InstallProgress,
  ModInstallResult,
  PackageDetails,
  ModdableGame,
  ModLaunchStatus,
  ModProfile,
  ModUpdate,
  PackageQuery,
  PackageSort,
  PackageSummary,
  ProfileMod,
} from "../../services/modding";

export type ModdableGameState = "ready" | "unsupported" | "unmatched";

export function moddableGameState(game: ModdableGame): ModdableGameState {
  if (game.ecosystemGameId === null) return "unmatched";
  return game.supported ? "ready" : "unsupported";
}

const stateOrder: Record<ModdableGameState, number> = { ready: 0, unsupported: 1, unmatched: 2 };

// Supported games first, the ones already set up for mods ahead of the rest.
export function sortModdableGames(games: readonly ModdableGame[]): ModdableGame[] {
  return [...games].sort((left, right) =>
    stateOrder[moddableGameState(left)] - stateOrder[moddableGameState(right)]
    || Number(right.binding !== null) - Number(left.binding !== null)
    || left.name.localeCompare(right.name, undefined, { sensitivity: "base" }));
}

function foldText(value: string): string {
  return value.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
}

export function filterEcosystemGames(games: readonly EcosystemGameSummary[], query: string): EcosystemGameSummary[] {
  const words = foldText(query).split(/\s+/).filter((word) => word.length > 0);
  if (words.length === 0) return [...games];
  return games.filter((game) => {
    const haystack = `${foldText(game.displayName)} ${game.id}`;
    return words.every((word) => haystack.includes(word));
  });
}

export const vanillaProfileValue = "";

export function profileOptions(profiles: readonly ModProfile[], selected: Language): { value: string; label: string }[] {
  return [
    { value: vanillaProfileValue, label: t("Vanilla (senza mod)", selected) },
    ...[...profiles]
      .sort((left, right) => left.name.localeCompare(right.name, undefined, { sensitivity: "base" }))
      .map((profile) => ({ value: profile.id, label: profile.name })),
  ];
}

export function activeProfile(game: ModdableGame | null, profiles: readonly ModProfile[]): ModProfile | null {
  const id = game?.binding?.activeProfileId ?? null;
  return id === null ? null : (profiles.find((profile) => profile.id === id) ?? null);
}

function hasForbiddenCharacter(value: string): boolean {
  return [...value].some((character) => {
    const code = character.codePointAt(0) ?? 0;
    return code < 0x20 || code === 0x7f || character === "/" || character === "\\";
  });
}

export function profileNameError(
  name: string,
  profiles: readonly ModProfile[],
  selected: Language,
  exceptId: string | null = null,
): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) return t("Inserisci un nome.", selected);
  if ([...trimmed].length > 64) return t("Il nome può avere al massimo 64 caratteri.", selected);
  if (hasForbiddenCharacter(trimmed)) return t("Il nome non può contenere / o \\.", selected);
  const lower = trimmed.toLowerCase();
  if (profiles.some((profile) => profile.id !== exceptId && profile.name.toLowerCase() === lower)) {
    return t("Esiste già un profilo con questo nome.", selected);
  }
  return null;
}

export function copyProfileName(name: string, profiles: readonly ModProfile[], selected: Language): string {
  const taken = new Set(profiles.map((profile) => profile.name.toLowerCase()));
  const base = t("{0} (copia)", selected, [name]);
  if (!taken.has(base.toLowerCase())) return base;
  for (let index = 2; ; index++) {
    const candidate = t("{0} (copia {1})", selected, [name, index]);
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
}

export interface PackageName { fullName: string; owner: string; name: string }

export function splitFullName(fullName: string): PackageName {
  const separator = fullName.indexOf("-");
  return separator <= 0
    ? { fullName, owner: "", name: fullName }
    : { fullName, owner: fullName.slice(0, separator), name: fullName.slice(separator + 1) };
}

// Package ids are `Owner-Name-1.2.3`; owners and names never contain dashes.
export function splitPackageId(packageId: string): { fullName: string; version: string } {
  const separator = packageId.lastIndexOf("-");
  return separator <= 0
    ? { fullName: packageId, version: "" }
    : { fullName: packageId.slice(0, separator), version: packageId.slice(separator + 1) };
}

// Thunderstore names use underscores for spaces.
export function packageTitle(name: string): string {
  return name.replaceAll("_", " ");
}

export function displayPackageId(packageId: string): string {
  const { fullName, version } = splitPackageId(packageId);
  const name = packageTitle(splitFullName(fullName).name);
  return version === "" ? name : `${name} ${version}`;
}

export function modKey(fullName: string): string {
  return fullName.toLowerCase();
}

export function indexByFullName<T extends { fullName: string }>(entries: readonly T[]): Map<string, T> {
  return new Map(entries.map((entry) => [modKey(entry.fullName), entry]));
}

export type InstalledState =
  | { kind: "missing" }
  | { kind: "installed"; version: string }
  | { kind: "outdated"; version: string; latestVersion: string };

export function installedState(
  fullName: string,
  installed: ReadonlyMap<string, ProfileMod>,
  updates: ReadonlyMap<string, ModUpdate>,
): InstalledState {
  const mod = installed.get(modKey(fullName));
  if (mod === undefined) return { kind: "missing" };
  const update = updates.get(modKey(fullName));
  return update === undefined
    ? { kind: "installed", version: mod.version }
    : { kind: "outdated", version: mod.version, latestVersion: update.latestVersion };
}

// BepInEx packs are named BepInExPack or BepInExPack_<Game> depending on the community.
export function isLoaderPackage(fullName: string): boolean {
  return /^bepinexpack(?:_|$)/i.test(splitFullName(fullName).name);
}

export function pickLoaderPackage(candidates: readonly PackageSummary[]): PackageSummary | null {
  const loaders = candidates.filter((candidate) => isLoaderPackage(candidate.fullName) && !candidate.isDeprecated);
  loaders.sort((left, right) =>
    Number(right.isPinned) - Number(left.isPinned) || right.totalDownloads - left.totalDownloads);
  return loaders[0] ?? null;
}

export type LaunchNotice = "none" | "vanilla" | "launch-unsupported" | "loader-missing";

export function launchNotice(status: ModLaunchStatus | null): LaunchNotice {
  if (status === null || !status.bound) return "none";
  if (status.activeProfileId === null) return "vanilla";
  if (!status.launchSupported) return "launch-unsupported";
  return status.loaderDeployed ? "none" : "loader-missing";
}

export interface BrowserFilters {
  text: string;
  category: string;
  sort: PackageSort;
  includeDeprecated: boolean;
  includeNsfw: boolean;
}

export const defaultBrowserFilters: BrowserFilters = {
  text: "",
  category: "",
  sort: "mostDownloads",
  includeDeprecated: false,
  includeNsfw: false,
};

export const browserPageSize = 30;

export function packageQuery(filters: BrowserFilters, offset: number, limit = browserPageSize): PackageQuery {
  const text = filters.text.trim();
  return {
    ...(text === "" ? {} : { text }),
    ...(filters.category === "" ? {} : { categories: [filters.category] }),
    includeDeprecated: filters.includeDeprecated,
    includeNsfw: filters.includeNsfw,
    sort: filters.sort,
    offset,
    limit,
  };
}

export function sortOptions(selected: Language): { value: PackageSort; label: string }[] {
  return [
    { value: "mostDownloads", label: t("Più scaricate", selected) },
    { value: "lastUpdated", label: t("Aggiornate di recente", selected) },
    { value: "topRated", label: t("Più votate", selected) },
    { value: "newest", label: t("Più recenti", selected) },
    { value: "alphabetical", label: t("Alfabetico (A-Z)", selected) },
  ];
}

export function appendPage(previous: readonly PackageSummary[], next: readonly PackageSummary[]): PackageSummary[] {
  const seen = new Set(previous.map((item) => modKey(item.fullName)));
  return [...previous, ...next.filter((item) => !seen.has(modKey(item.fullName)))];
}

const catalogFreshSeconds = 60 * 60;

export function isCatalogStale(syncedAt: number | null, nowMilliseconds: number): boolean {
  return syncedAt === null || nowMilliseconds / 1000 - syncedAt > catalogFreshSeconds;
}

export function catalogPercent(chunksDone: number, chunksTotal: number): number {
  return chunksTotal <= 0 ? 0 : Math.round((chunksDone / chunksTotal) * 100);
}

// Bytes while a download reports its size, otherwise the share of packages already handled.
export function installPercent(progress: InstallProgress): number {
  if (progress.stage === "done") return 100;
  if (progress.stage === "downloading" && progress.totalBytes !== null && progress.totalBytes > 0) {
    return Math.min(100, ((progress.downloadedBytes ?? 0) / progress.totalBytes) * 100);
  }
  return progress.total > 0 ? Math.min(100, (Math.max(0, progress.current - 1) / progress.total) * 100) : 0;
}

export function installStageLabel(progress: InstallProgress, selected: Language): string {
  switch (progress.stage) {
    case "resolving": return t("Risoluzione delle dipendenze...", selected);
    case "downloading": return t("Download in corso", selected);
    case "extracting": return t("Estrazione", selected);
    case "installing": return t("Installazione", selected);
    case "done": return t("Completato", selected);
  }
}

export function formatCount(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 }).format(value);
}

// Backend errors are English; the ones users meet often get an Italian source string.
const knownErrors: Record<string, string> = {
  "Stop the game before changing its installation": "Chiudi il gioco prima di modificarne l'installazione.",
  "Mods are already being downloaded for this profile": "È già in corso un download di mod per questo profilo.",
  "Download cancelled": "Download annullato.",
  "The mod catalog of this game has not been downloaded yet. Refresh it first": "Il catalogo delle mod di questo gioco non è ancora stato scaricato. Aggiornalo prima.",
};

export function describeModdingError(message: string, selected: Language): string {
  const known = knownErrors[message.trim().replace(/\.$/, "")];
  return known === undefined ? message : t(known, selected);
}

export function isCancellation(message: string): boolean {
  return message.trim().replace(/\.$/, "") === "Download cancelled";
}

// Installed mods whose installed version lists `fullName` among its dependencies.
export function dependentsOf(
  fullName: string,
  mods: readonly ProfileMod[],
  details: Readonly<Record<string, PackageDetails | null>>,
): string[] {
  const target = modKey(fullName);
  return mods.filter((mod) => {
    if (modKey(mod.fullName) === target) return false;
    const version = details[modKey(mod.fullName)]?.versions.find((entry) => entry.versionNumber === mod.version);
    return version?.dependencies.some((dependency) => modKey(splitPackageId(dependency).fullName) === target) ?? false;
  }).map((mod) => mod.fullName);
}

export interface ResultNotice { tone: "success" | "info" | "warning"; message: string; items: string[] }

export function installNotice(result: ModInstallResult, selected: Language): ResultNotice {
  if (result.warnings.length > 0) {
    return { tone: "warning", message: t("Installazione completata con avvisi.", selected), items: result.warnings };
  }
  if (result.installed.length === 0) return { tone: "info", message: t("Le mod richieste sono già installate.", selected), items: [] };
  const names = result.installed.map(displayPackageId);
  return result.installed.length === 1
    ? { tone: "success", message: t("{0} installata.", selected, [names[0]]), items: [] }
    : { tone: "success", message: t("{0} mod installate.", selected, [result.installed.length]), items: names };
}
