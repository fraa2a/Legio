import { t, type Language } from "../../i18n";
import { toMessage } from "../../utils/errors";
import type {
  CategoryMode,
  EcosystemGameSummary,
  InstallProgress,
  ModInstallResult,
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

export const maxProfileNameLength = 64;

export function profileNameError(
  name: string,
  profiles: readonly ModProfile[],
  selected?: Language,
  exceptId: string | null = null,
): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) return t("Inserisci un nome.", selected);
  if ([...trimmed].length > maxProfileNameLength) return t("Il nome può avere al massimo {0} caratteri.", selected, [maxProfileNameLength]);
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

export type LaunchNotice = "none" | "launch-unsupported" | "loader-missing";

// `loaderDeployed` only turns true at launch, so the profile itself decides whether BepInEx is missing.
export function launchNotice(status: ModLaunchStatus | null): LaunchNotice {
  if (status === null || !status.bound || status.activeProfileId === null) return "none";
  if (!status.launchSupported) return "launch-unsupported";
  return status.preloaderFound ? "none" : "loader-missing";
}

export interface BrowserFilters {
  text: string;
  categories: string[];
  categoryMode: CategoryMode;
  excludedCategories: string[];
  sort: PackageSort;
  includeDeprecated: boolean;
  includeNsfw: boolean;
}

export const defaultBrowserFilters: BrowserFilters = {
  text: "",
  categories: [],
  categoryMode: "any",
  excludedCategories: [],
  sort: "mostDownloads",
  includeDeprecated: false,
  includeNsfw: false,
};

export const browserPageSize = 30;

export function packageQuery(filters: BrowserFilters, offset: number, limit = browserPageSize): PackageQuery {
  const text = filters.text.trim();
  return {
    ...(text === "" ? {} : { text }),
    ...(filters.categories.length === 0 ? {} : { categories: filters.categories, categoryMode: filters.categoryMode }),
    ...(filters.excludedCategories.length === 0 ? {} : { excludedCategories: filters.excludedCategories }),
    includeDeprecated: filters.includeDeprecated,
    includeNsfw: filters.includeNsfw,
    sort: filters.sort,
    offset,
    limit,
  };
}

export type CategoryState = "neutral" | "included" | "excluded";

export function categoryState(filters: BrowserFilters, category: string): CategoryState {
  if (filters.categories.includes(category)) return "included";
  return filters.excludedCategories.includes(category) ? "excluded" : "neutral";
}

// Chips cycle neutral, included, excluded, neutral.
export function cycleCategory(filters: BrowserFilters, category: string): BrowserFilters {
  const without = (list: string[]) => list.filter((entry) => entry !== category);
  const next = { ...filters, categories: without(filters.categories), excludedCategories: without(filters.excludedCategories) };
  switch (categoryState(filters, category)) {
    case "neutral": return { ...next, categories: [...next.categories, category] };
    case "included": return { ...next, excludedCategories: [...next.excludedCategories, category] };
    case "excluded": return next;
  }
}

export function activeFilterCount(filters: BrowserFilters): number {
  return filters.categories.length + filters.excludedCategories.length
    + Number(filters.includeDeprecated) + Number(filters.includeNsfw);
}

// Keeps the search text and the sort order.
export function clearFilterOptions(filters: BrowserFilters): BrowserFilters {
  return {
    ...filters,
    categories: [],
    categoryMode: "any",
    excludedCategories: [],
    includeDeprecated: false,
    includeNsfw: false,
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
  "The mod loader of this game is not in the mod catalog": "Il loader di mod di questo gioco non è presente nel catalogo mod.",
  "Mods are being changed in the active mod profile. Wait for it to finish and launch again": "Le mod del profilo attivo sono in fase di modifica. Attendi il termine e avvia di nuovo.",
  "The active mod profile has no BepInEx loader. Install BepInExPack in the profile first": "Il profilo mod attivo non ha BepInEx. Installalo dal gestore delle mod o scegli Vanilla per avviare senza mod.",
  "The config file changed on disk; reload it": "Il file è cambiato sul disco: ricaricalo prima di salvare.",
  "The link is not a web address": "Il collegamento non è un indirizzo web.",
  "The mod cache is being cleared. Wait for it to finish and try again": "La cache delle mod è in fase di eliminazione. Attendi il termine e riprova.",
  "The profile is being changed by another operation": "Il profilo è in uso da un'altra operazione.",
};

// Errors carrying a name; the first group fills the Italian `{0}`.
const knownPatterns: [RegExp, string][] = [
  [/^(.+?) already exists in the game folder and was not installed by Legio\b/, "{0} è già nella cartella del gioco e non è stato installato da Legio. Spostalo o rimuovilo per avviare con le mod."],
  [/^A mod profile named "(.*)" already exists\.?$/, "Esiste già un profilo chiamato \"{0}\". Scegli un altro nome."],
  [/^These mods could not be found, so the profile was not changed: (.+)$/, "Il profilo non è stato modificato perché queste mod non sono state trovate: {0}"],
];

export function describeModdingError(message: string, selected?: Language): string {
  const trimmed = message.trim();
  const known = knownErrors[trimmed.replace(/\.$/, "")];
  if (known !== undefined) return t(known, selected);
  for (const [pattern, translation] of knownPatterns) {
    const match = pattern.exec(trimmed);
    if (match !== null) return t(translation, selected, [match[1]]);
  }
  return message;
}

function isCancellation(message: string): boolean {
  return message.trim().replace(/\.$/, "") === "Download cancelled";
}

export type ModdingFailure = { kind: "cancelled"; message: string } | { kind: "error"; message: string };

// A cancelled install is reported as a toast, every other failure inline.
export function classifyModdingError(error: unknown, selected?: Language): ModdingFailure {
  const message = toMessage(error);
  return isCancellation(message)
    ? { kind: "cancelled", message: t("Installazione annullata.", selected) }
    : { kind: "error", message: describeModdingError(message, selected) };
}

// Installed mods that list `fullName` among their declared dependencies.
export function dependentsOf(fullName: string, mods: readonly ProfileMod[]): string[] {
  const target = modKey(fullName);
  return mods
    .filter((mod) => modKey(mod.fullName) !== target && mod.dependencies.some((dependency) => modKey(dependency) === target))
    .map((mod) => mod.fullName);
}

export interface DependencyIssues { missing: string[]; disabled: string[] }

// Per mod (by `modKey`), the declared dependencies absent from the profile and the ones installed but off.
// A disabled mod needs nothing, so it is only checked for missing packages.
export function dependencyIssues(mods: readonly ProfileMod[]): Map<string, DependencyIssues> {
  const installed = indexByFullName(mods);
  const issues = new Map<string, DependencyIssues>();
  for (const mod of mods) {
    const found: DependencyIssues = { missing: [], disabled: [] };
    for (const dependency of mod.dependencies) {
      const key = modKey(dependency);
      if (key === modKey(mod.fullName)) continue;
      const target = installed.get(key);
      if (target === undefined) found.missing.push(dependency);
      else if (mod.enabled && !target.enabled) found.disabled.push(target.fullName);
    }
    if (found.missing.length > 0 || found.disabled.length > 0) issues.set(modKey(mod.fullName), found);
  }
  return issues;
}

export function dependencyIssueLabels(issues: DependencyIssues, selected: Language): string[] {
  const title = (fullName: string) => packageTitle(splitFullName(fullName).name);
  return [
    ...issues.missing.map((name) => t("Dipendenza mancante: {0}", selected, [title(name)])),
    ...issues.disabled.map((name) => t("Dipendenza disattivata: {0}", selected, [title(name)])),
  ];
}

// Disabled mods to switch on, dependencies first, so that `fullNames` end up enabled with everything they need.
export function enablePlan(fullNames: readonly string[], mods: readonly ProfileMod[]): string[] {
  const installed = indexByFullName(mods);
  const visited = new Set<string>();
  const order: string[] = [];
  const visit = (fullName: string): void => {
    const mod = installed.get(modKey(fullName));
    if (mod === undefined || visited.has(modKey(mod.fullName))) return;
    visited.add(modKey(mod.fullName));
    for (const dependency of mod.dependencies) visit(dependency);
    if (!mod.enabled) order.push(mod.fullName);
  };
  fullNames.forEach(visit);
  return order;
}

// Enabled mods that need `fullName`, directly or through other enabled mods, the most dependent first.
export function enabledDependents(fullName: string, mods: readonly ProfileMod[]): string[] {
  const enabled = mods.filter((mod) => mod.enabled);
  const visited = new Set([modKey(fullName)]);
  const order: string[] = [];
  const visit = (name: string): void => {
    for (const dependent of dependentsOf(name, enabled)) {
      if (visited.has(modKey(dependent))) continue;
      visited.add(modKey(dependent));
      visit(dependent);
      order.push(dependent);
    }
  };
  visit(fullName);
  return order;
}

// What a one-click fix does: install the missing packages, switch on the disabled ones with their own needs.
export function dependencyFix(issues: DependencyIssues, mods: readonly ProfileMod[]): { install: string[]; enable: string[] } {
  return { install: issues.missing, enable: enablePlan(issues.disabled, mods) };
}

export type InstalledSort = "order" | "name" | "date";
export type DisabledPosition = "mixed" | "first" | "last";

export interface InstalledFilters {
  text: string;
  sort: InstalledSort;
  descending: boolean;
  disabled: DisabledPosition;
  onlyUpdates: boolean;
}

export const defaultInstalledFilters: InstalledFilters = {
  text: "",
  sort: "order",
  descending: false,
  disabled: "mixed",
  onlyUpdates: false,
};

export function installedSortOptions(selected: Language): { value: InstalledSort; label: string }[] {
  return [
    { value: "order", label: t("Ordine di installazione", selected) },
    { value: "name", label: t("Nome", selected) },
    { value: "date", label: t("Data di installazione", selected) },
  ];
}

export function disabledPositionOptions(selected: Language): { value: DisabledPosition; label: string }[] {
  return [
    { value: "mixed", label: t("Disattivate: in ordine", selected) },
    { value: "first", label: t("Disattivate: per prime", selected) },
    { value: "last", label: t("Disattivate: per ultime", selected) },
  ];
}

export function hasInstalledFilters(filters: InstalledFilters): boolean {
  return filters.text.trim() !== "" || filters.onlyUpdates;
}

function matchesText(mod: ProfileMod, words: readonly string[]): boolean {
  const { owner, name } = splitFullName(mod.fullName);
  const haystack = foldText(`${owner} ${name} ${packageTitle(name)}`);
  return words.every((word) => haystack.includes(word));
}

// The loader row stays on top whatever the sort; the other rows are searched, filtered and ordered.
export function arrangeInstalledMods(
  mods: readonly ProfileMod[],
  filters: InstalledFilters,
  updates: ReadonlySet<string>,
): ProfileMod[] {
  const words = foldText(filters.text).split(/\s+/).filter((word) => word.length > 0);
  const kept = mods.filter((mod) => matchesText(mod, words) && (!filters.onlyUpdates || updates.has(modKey(mod.fullName))));
  const byOrder = (left: ProfileMod, right: ProfileMod) => left.installOrder - right.installOrder;
  const byName = (left: ProfileMod, right: ProfileMod) =>
    packageTitle(splitFullName(left.fullName).name).localeCompare(packageTitle(splitFullName(right.fullName).name), undefined, { sensitivity: "base" })
    || left.fullName.localeCompare(right.fullName, undefined, { sensitivity: "base" });
  const byDate = (left: ProfileMod, right: ProfileMod) => left.installedAt - right.installedAt || byOrder(left, right);
  const compare = filters.sort === "name" ? byName : filters.sort === "date" ? byDate : byOrder;
  const direction = filters.descending ? -1 : 1;
  const loaders = kept.filter((mod) => mod.isLoader).sort(byOrder);
  const sorted = kept.filter((mod) => !mod.isLoader).sort((left, right) => direction * compare(left, right));
  if (filters.disabled === "mixed") return [...loaders, ...sorted];
  const disabledFirst = filters.disabled === "first";
  return [
    ...loaders,
    ...sorted.filter((mod) => mod.enabled !== disabledFirst),
    ...sorted.filter((mod) => mod.enabled === disabledFirst),
  ];
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
