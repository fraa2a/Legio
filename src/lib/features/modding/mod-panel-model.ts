import { t, type Language } from "../../i18n";
import type { PackageSummary, PackageVersionDetails } from "../../services/modding";
import { modKey, packageTitle, splitFullName } from "./modding-model";

export interface ModTab {
  key: string;
  fullName: string;
  // Known when the tab is opened from a search result; dependency tabs start without it.
  summary: PackageSummary | null;
}

export interface ModTabs {
  tabs: readonly ModTab[];
  active: string | null;
  // Tab keys from the least to the most recently shown.
  recent: readonly string[];
}

export const modTabLimit = 8;

export const emptyModTabs: ModTabs = { tabs: [], active: null, recent: [] };

function markRecent(recent: readonly string[], key: string): string[] {
  return [...recent.filter((entry) => entry !== key), key];
}

// Opens a mod in a new tab, or shows its tab when it is already open.
export function openModTab(state: ModTabs, fullName: string, summary: PackageSummary | null = null): ModTabs {
  const key = modKey(fullName);
  const existing = state.tabs.find((tab) => tab.key === key);
  if (existing !== undefined) {
    const tabs = summary !== null && existing.summary === null
      ? state.tabs.map((tab) => (tab.key === key ? { ...tab, summary } : tab))
      : state.tabs;
    return { tabs, active: key, recent: markRecent(state.recent, key) };
  }
  let tabs = [...state.tabs, { key, fullName, summary }];
  let recent = markRecent(state.recent, key);
  if (tabs.length > modTabLimit) {
    const [oldest] = recent;
    tabs = tabs.filter((tab) => tab.key !== oldest);
    recent = recent.slice(1);
  }
  return { tabs, active: key, recent };
}

export function focusModTab(state: ModTabs, key: string): ModTabs {
  if (!state.tabs.some((tab) => tab.key === key)) return state;
  return { ...state, active: key, recent: markRecent(state.recent, key) };
}

// Closing the shown tab shows its right neighbour, else its left one, as browsers do.
export function closeModTab(state: ModTabs, key: string): ModTabs {
  const index = state.tabs.findIndex((tab) => tab.key === key);
  if (index < 0) return state;
  const tabs = state.tabs.filter((tab) => tab.key !== key);
  const recent = state.recent.filter((entry) => entry !== key);
  if (state.active !== key) return { tabs, active: state.active, recent };
  const next = tabs[Math.min(index, tabs.length - 1)]?.key ?? null;
  return { tabs, active: next, recent: next === null ? recent : markRecent(recent, next) };
}

export function setModTabSummary(state: ModTabs, summary: PackageSummary): ModTabs {
  const key = modKey(summary.fullName);
  const tab = state.tabs.find((entry) => entry.key === key);
  if (tab === undefined || tab.summary !== null) return state;
  return { ...state, tabs: state.tabs.map((entry) => (entry === tab ? { ...entry, summary } : entry)) };
}

// The tab key reached from `key` by arrow keys, wrapping around the strip.
export function adjacentModTab(state: ModTabs, key: string, offset: number): string | null {
  const count = state.tabs.length;
  const index = state.tabs.findIndex((tab) => tab.key === key);
  if (count === 0 || index < 0) return null;
  return state.tabs[(((index + offset) % count) + count) % count].key;
}

export function modTabTitle(tab: ModTab): string {
  return packageTitle(tab.summary?.name ?? splitFullName(tab.fullName).name);
}

export function versionChoices(versions: readonly PackageVersionDetails[], language: Language): { value: string; label: string }[] {
  return versions.map((entry, index) => ({
    value: entry.versionNumber,
    label: index === 0 ? t("{0} (ultima)", language, [entry.versionNumber]) : entry.versionNumber,
  }));
}

export function installActionLabel(installedVersion: string | null, version: string, isLatest: boolean, language: Language): string {
  if (installedVersion === version) return t("Installata", language);
  if (installedVersion !== null && isLatest) return t("Aggiorna a {0}", language, [version]);
  return isLatest && installedVersion === null ? t("Installa", language) : t("Installa la versione {0}", language, [version]);
}
