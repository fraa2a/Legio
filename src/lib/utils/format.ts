import { get } from "svelte/store";
import { language, type Language } from "../i18n";

const byteUnits = ["KB", "MB", "GB", "TB"];

// Follows the app language but keeps the system region (for example en-GB) when it speaks that language.
export function regionalLocale(selected: Language, preferences: readonly string[] = typeof navigator === "undefined" ? [] : navigator.languages): string {
  return preferences.find((tag) => tag.toLowerCase().split(/[-_]/)[0] === selected) ?? selected;
}

export function formatBytes(value: number, selected: Language = get(language)): string {
  if (value < 1024) return `${value} B`;
  let size = value / 1024;
  let unit = 0;
  while (size >= 1024 && unit < byteUnits.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toLocaleString(regionalLocale(selected), { minimumFractionDigits: 1, maximumFractionDigits: 1 })} ${byteUnits[unit]}`;
}

export function formatDate(value: string | number | Date, selected: Language = get(language), options?: Intl.DateTimeFormatOptions): string {
  return new Date(value).toLocaleDateString(regionalLocale(selected), options);
}

// The backend reports cache freshness as epoch seconds.
export function formatDateTime(epochSeconds: number, selected: Language = get(language)): string {
  return new Date(epochSeconds * 1000).toLocaleString(regionalLocale(selected), {
    dateStyle: "short",
    timeStyle: "short",
  });
}
