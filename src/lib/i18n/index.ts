import { get, writable } from "svelte/store";
import english from "./en.json";

export type Language = "it" | "en";
export type LanguagePreference = "system" | Language;

// The backend resolves "system" so the interface, tray, notifications and Steam details agree.
let systemLanguage: Language = "en";
let current: LanguagePreference = "system";
export const language = writable<Language>(systemLanguage);

export function setSystemLanguage(resolved: Language): void {
  systemLanguage = resolved;
  setLanguage(current);
}

export function setLanguage(preference: LanguagePreference): void {
  current = preference;
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
