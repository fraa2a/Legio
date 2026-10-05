import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import presets from "./theme-presets.json";
import type { Settings, Theme } from "./local-state";

export interface Palette {
  background: string;
  surface: string;
  raised: string;
  text: string;
  muted: string;
  accent: string;
}

export interface CustomTheme {
  id: string;
  name: string;
  scheme: "dark" | "light";
  palette: Palette;
}

export interface Appearance {
  customThemes: CustomTheme[];
  customThemeId: string | null;
  background: string | null;
  backgroundBlur: number;
  backgroundOpacity: number;
  animatedBackground: "none" | "particles" | "dither";
  animatedOpacity: number;
  surfaceOpacity: number;
  surfaceBlur: number;
  dialogOpacity: number;
  dialogBlur: number;
  transparent: boolean;
}

export const themePresets = presets as CustomTheme[];

export function defaultAppearance(): Appearance {
  return {
    customThemes: [], customThemeId: null, background: null,
    backgroundBlur: 0, backgroundOpacity: 60, surfaceOpacity: 90, surfaceBlur: 0,
    dialogOpacity: 90, dialogBlur: 0, transparent: false,
    animatedBackground: "none", animatedOpacity: 65,
  };
}

export function activePalette(theme: Theme, appearance: Appearance, prefersDark: boolean): CustomTheme {
  if (theme === "custom") {
    const custom = appearance.customThemes.find((entry) => entry.id === appearance.customThemeId);
    if (custom) return custom;
  }
  const id = theme === "system" ? (prefersDark ? "dark" : "light") : theme;
  return themePresets.find((preset) => preset.id === id) ?? themePresets[0];
}

function channels(color: string): number[] {
  return [1, 3, 5].map((index) => Number.parseInt(color.slice(index, index + 2), 16));
}

function mix(first: string, second: string, ratio: number): string {
  const a = channels(first);
  const b = channels(second);
  return "#" + a.map((value, index) => Math.round(value * (1 - ratio) + b[index] * ratio).toString(16).padStart(2, "0")).join("");
}

function luminance(color: string): number {
  const linear = channels(color).map((value) => {
    const srgb = value / 255;
    return srgb <= 0.04045 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
  });
  return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
}

export function accentText(color: string): string {
  const value = luminance(color);
  return (value + 0.05) / 0.05 >= 1.05 / (value + 0.05) ? "#000000" : "#ffffff";
}

// Invalid drafts stay in the editor; keep the surrounding app readable.
export function readablePalette(preset: CustomTheme): boolean {
  const p = preset.palette;
  const text = luminance(p.text);
  return [p.background, p.surface, p.raised].every((color) => {
    const background = luminance(color);
    return (text > background) === (preset.scheme === "dark")
      && (Math.max(text, background) + 0.05) / (Math.min(text, background) + 0.05) >= 4.5;
  });
}

export function paletteVariables(preset: CustomTheme): Record<string, string> {
  const p = preset.palette;
  const dark = preset.scheme === "dark";
  let colors = dark
    ? [p.text, mix(p.text, p.muted, 0.1), mix(p.text, p.muted, 0.25), mix(p.text, p.muted, 0.5), p.muted, mix(p.muted, p.raised, 0.35), mix(p.muted, p.raised, 0.65), mix(p.raised, p.muted, 0.15), p.raised, p.surface, p.background]
    : [p.surface, p.surface, p.background, mix(p.background, p.muted, 0.2), mix(p.background, p.muted, 0.55), p.muted, mix(p.muted, p.text, 0.25), mix(p.muted, p.text, 0.5), mix(p.muted, p.text, 0.75), p.text, p.text];
  if (preset.id === "dark" || preset.id === "light") {
    colors = ["#fafafa", "#f4f4f5", "#e4e4e7", "#d4d4d8", "#a1a1aa", "#71717a", "#52525b", "#3f3f46", "#27272a", "#18181b", "#09090b"];
  }
  const variables: Record<string, string> = {
    "--legio-background": p.background, "--legio-surface": p.surface,
    "--legio-raised": p.raised, "--legio-text": p.text, "--legio-muted": p.muted,
    "--legio-accent": p.accent, "--legio-accent-text": accentText(p.accent),
    "--legio-focus": p.accent,
    "--color-white": dark ? p.text : "#ffffff",
  };
  [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950].forEach((shade, index) => {
    variables["--color-zinc-" + shade] = colors[index];
  });
  variables["--color-black"] = dark ? p.background : "#000000";
  return variables;
}

export function applyAppearance(
  root: HTMLElement, theme: Theme, appearance: Appearance, prefersDark: boolean, hyprland: boolean,
): void {
  const selected = activePalette(theme, appearance, prefersDark);
  const palette = readablePalette(selected) ? selected : themePresets.find((preset) => preset.id === selected.scheme) ?? themePresets[0];
  root.dataset.theme = palette.scheme;
  root.dataset.palette = theme;
  root.dataset.transparent = String(appearance.transparent && hyprland);
  root.style.colorScheme = palette.scheme;
  for (const [key, value] of Object.entries(paletteVariables(palette))) root.style.setProperty(key, value);
  root.style.setProperty("--legio-image-blur", appearance.backgroundBlur + "px");
  root.style.setProperty("--legio-image-opacity", String(appearance.backgroundOpacity / 100));
  root.style.setProperty("--legio-surface-opacity", String(appearance.surfaceOpacity / 100));
  root.style.setProperty("--legio-surface-blur", (appearance.surfaceOpacity < 100 ? appearance.surfaceBlur : 0) + "px");
  root.style.setProperty("--legio-dialog-opacity", String(appearance.dialogOpacity / 100));
  root.style.setProperty("--legio-dialog-blur", (appearance.dialogOpacity < 100 ? appearance.dialogBlur : 0) + "px");
}

export async function chooseBackground(): Promise<string | null> {
  const path = await open({
    title: t("Scegli uno sfondo"), multiple: false,
    filters: [{ name: t("Immagini"), extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  return path === null ? null : invoke<string>("import_theme_background", { source: path });
}

export async function importTheme(): Promise<Settings | null> {
  const path = await open({
    title: t("Importa tema JSON"), multiple: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  return path === null ? null : invoke<Settings>("import_theme_file", { source: path });
}

export async function exportTheme(): Promise<boolean> {
  const path = await save({
    title: t("Esporta tema JSON"), defaultPath: "legio-theme.json",
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (path === null) return false;
  await invoke("export_theme_file", {
    destination: path, prefersDark: window.matchMedia("(prefers-color-scheme: dark)").matches,
  });
  return true;
}

export function cleanupBackgrounds(): Promise<void> {
  return invoke("cleanup_theme_backgrounds");
}

export async function loadBackground(id: string): Promise<string> {
  const bytes = await invoke<ArrayBuffer>("get_theme_background", { id });
  return URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
}
