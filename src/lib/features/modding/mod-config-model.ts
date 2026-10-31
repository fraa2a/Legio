import { t, type Language } from "../../i18n";
import type { BepInExConfig, ConfigEdit, ConfigEntry, ConfigFileEntry } from "../../services/mod-config";

export type EntryControl = "boolean" | "choice" | "number" | "text";

export const maxConfigTextBytes = 2 * 1024 * 1024;
export const fileListPageSize = 200;
const largeConfigEntries = 60;

const integerTypes = new Set(["byte", "sbyte", "int16", "uint16", "int32", "uint32", "int64", "uint64", "int", "uint", "short", "ushort", "long", "ulong"]);
const decimalTypes = new Set(["single", "double", "decimal", "float"]);
const numberPattern = /^-?\d+(\.\d+)?([eE][+-]?\d+)?$/;
const integerPattern = /^-?\d+$/;

export interface ConfigFileView {
  file: ConfigFileEntry;
  directory: string;
  name: string;
}

export function splitConfigPath(relativePath: string): { directory: string; name: string } {
  const normalized = relativePath.replaceAll("\\", "/");
  const index = normalized.lastIndexOf("/");
  return index < 0 ? { directory: "", name: normalized } : { directory: normalized.slice(0, index), name: normalized.slice(index + 1) };
}

export function filterConfigFiles(files: readonly ConfigFileEntry[], query: string): ConfigFileView[] {
  const needle = query.trim().toLowerCase();
  return files
    .filter((file) => needle.length === 0 || file.relativePath.toLowerCase().includes(needle))
    .map((file) => ({ file, ...splitConfigPath(file.relativePath) }))
    .sort((a, b) => a.file.relativePath.localeCompare(b.file.relativePath, undefined, { sensitivity: "base", numeric: true }));
}

const typeName = (entry: ConfigEntry): string => (entry.settingType ?? "").trim().toLowerCase();

export function isIntegerEntry(entry: ConfigEntry): boolean {
  return integerTypes.has(typeName(entry));
}

function allowsMultipleChoices(entry: ConfigEntry): boolean {
  return /multiple values can be set/i.test(entry.description ?? "");
}

export function entryControl(entry: ConfigEntry): EntryControl {
  const type = typeName(entry);
  if ((type === "boolean" || type === "bool") && /^(true|false)$/i.test(entry.value.trim())) return "boolean";
  if (entry.acceptableValues.length > 0 && !allowsMultipleChoices(entry)) return "choice";
  if ((integerTypes.has(type) || decimalTypes.has(type)) && numberPattern.test(entry.value.trim())) return "number";
  return "text";
}

// The current value stays selectable even when the file holds something the list does not mention.
export function choiceOptions(entry: ConfigEntry, current: string): { value: string; label: string }[] {
  const values = entry.acceptableValues.includes(current) ? entry.acceptableValues : [current, ...entry.acceptableValues];
  return values.map((value) => ({ value, label: value }));
}

export function rangeHint(entry: ConfigEntry, language?: Language): string | null {
  if (entry.acceptableRange === null) return null;
  return t("Valori da {0} a {1}", language, [entry.acceptableRange[0], entry.acceptableRange[1]]);
}

export function validateEntry(entry: ConfigEntry, value: string, language?: Language): string | null {
  if (entryControl(entry) !== "number") return null;
  const trimmed = value.trim();
  if (!numberPattern.test(trimmed)) return t("Inserisci un numero", language);
  if (isIntegerEntry(entry) && !integerPattern.test(trimmed)) return t("Inserisci un numero intero", language);
  if (entry.acceptableRange !== null) {
    const number = Number(trimmed);
    const [min, max] = entry.acceptableRange.map(Number);
    if (Number.isFinite(min) && number < min) return t("Il valore minimo è {0}", language, [entry.acceptableRange[0]]);
    if (Number.isFinite(max) && number > max) return t("Il valore massimo è {0}", language, [entry.acceptableRange[1]]);
  }
  return null;
}

// Files may repeat section and key names, so entries are told apart by their line.
export const entryId = (section: string, entry: ConfigEntry): string => `${section}\u0000${entry.line}`;

export type ConfigChanges = Record<string, string>;

export function currentValue(section: string, entry: ConfigEntry, changes: ConfigChanges): string {
  return changes[entryId(section, entry)] ?? entry.value;
}

export function setChange(changes: ConfigChanges, section: string, entry: ConfigEntry, value: string): ConfigChanges {
  const id = entryId(section, entry);
  const next = { ...changes };
  if (value === entry.value) delete next[id];
  else next[id] = value;
  return next;
}

export function collectEdits(config: BepInExConfig, changes: ConfigChanges): ConfigEdit[] {
  const edits: ConfigEdit[] = [];
  for (const section of config.sections) {
    for (const entry of section.entries) {
      const value = changes[entryId(section.name, entry)];
      if (value !== undefined && value !== entry.value) edits.push({ section: section.name, key: entry.key, value, line: entry.line });
    }
  }
  return edits;
}

export function findInvalidEntries(config: BepInExConfig, changes: ConfigChanges): number {
  let invalid = 0;
  for (const section of config.sections) {
    for (const entry of section.entries) {
      const value = changes[entryId(section.name, entry)];
      if (value !== undefined && validateEntry(entry, value) !== null) invalid += 1;
    }
  }
  return invalid;
}

// `id` is the position in the file, since section names can repeat.
export interface ConfigSectionView {
  id: string;
  name: string;
  entries: ConfigEntry[];
}

export function sectionChangeCount(section: ConfigSectionView, changes: ConfigChanges): number {
  return section.entries.filter((entry) => changes[entryId(section.name, entry)] !== undefined).length;
}

export function filterSections(config: BepInExConfig, query: string): ConfigSectionView[] {
  const needle = query.trim().toLowerCase();
  return config.sections
    .map((section, index) => ({
      id: String(index),
      name: section.name,
      entries: needle.length === 0 ? section.entries : section.entries.filter((entry) =>
        section.name.toLowerCase().includes(needle) || entry.key.toLowerCase().includes(needle) || (entry.description ?? "").toLowerCase().includes(needle)),
    }))
    .filter((section) => needle.length === 0 || section.entries.length > 0);
}

// Big files start folded so the first paint stays cheap.
export function initialOpenSections(config: BepInExConfig): string[] {
  const total = config.sections.reduce((sum, section) => sum + section.entries.length, 0);
  const open = total > largeConfigEntries ? config.sections.slice(0, 1) : config.sections;
  return open.map((_section, index) => String(index));
}

export function textSizeError(text: string, language?: Language): string | null {
  // A UTF-8 character takes at most three bytes in the BMP, so short texts cannot exceed the limit.
  if (text.length * 3 <= maxConfigTextBytes) return null;
  return new TextEncoder().encode(text).length > maxConfigTextBytes
    ? t("Il file supera il limite di 2 MiB", language)
    : null;
}

export function formatBoolean(checked: boolean): string {
  return checked ? "true" : "false";
}
