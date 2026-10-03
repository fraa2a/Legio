import { get, writable } from "svelte/store";
import english from "./en.json";

export type Language = "it" | "en";
export type LanguagePreference = "system" | Language;

export function resolveLanguage(preferences: readonly string[]): Language {
  for (const locale of preferences) {
    const primary = locale.toLowerCase().split(/[-_]/)[0];
    if (primary === "it" || primary === "en") return primary;
  }
  return "en";
}

const systemLanguage = resolveLanguage(typeof navigator === "undefined" ? [] : navigator.languages);
export const language = writable<Language>(systemLanguage);

export function setLanguage(preference: LanguagePreference): void {
  const resolved = preference === "system" ? systemLanguage : preference;
  language.set(resolved);
  if (typeof document !== "undefined") document.documentElement.lang = resolved;
}

export function t(message: string, selected: Language = get(language), values: unknown[] = []): string {
  const trimmed = message.trim();
  const translated = selected === "en" ? (english as Record<string, string>)[trimmed] ?? trimmed : trimmed;
  const text = message.replace(trimmed, translated);
  return text.replace(/\{(\d+)\}/g, (_, index: string) => String(values[Number(index)] ?? ""));
}
