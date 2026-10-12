import { invoke } from "./invoke";
import { open } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import presets from "./theme-presets.json";
import type { Theme } from "./local-state";

export interface Palette {
  background: string;
  surface: string;
  raised: string;
  text: string;
  muted: string;
  accent: string;
}

export interface ThemePreset {
  id: string;
  name: string;
  scheme: "dark" | "light";
  palette: Palette;
}

export interface DitherSettings {
  waveSpeed: number;
  waveFrequency: number;
  waveAmplitude: number;
  waveColor: string | null;
  backgroundColor: string;
  colorNum: number;
  pixelSize: number;
  disableAnimation: boolean;
  enableMouseInteraction: boolean;
  mouseRadius: number;
}

export interface Appearance {
  background: string | null;
  backgroundBlur: number;
  backgroundOpacity: number;
  animatedBackground: "none" | "particles" | "dither";
  animatedOpacity: number;
  dither: DitherSettings;
  surfaceOpacity: number;
  surfaceBlur: number;
  dialogOpacity: number;
  dialogBlur: number;
  transparent: boolean;
}

export const themePresets = presets as ThemePreset[];

export function defaultDither(): DitherSettings {
  return {
    waveSpeed: 0.05, waveFrequency: 3, waveAmplitude: 0.3,
    waveColor: null, backgroundColor: "#000000", colorNum: 4, pixelSize: 2,
    disableAnimation: false, enableMouseInteraction: true, mouseRadius: 1,
  };
}

export function defaultAppearance(): Appearance {
  return {
    background: null, backgroundBlur: 0, backgroundOpacity: 60,
    animatedBackground: "none", animatedOpacity: 65, dither: defaultDither(),
    surfaceOpacity: 90, surfaceBlur: 0, dialogOpacity: 90, dialogBlur: 0, transparent: false,
  };
}

export function activePalette(theme: Theme, prefersDark: boolean): ThemePreset {
  const id = theme === "system" ? (prefersDark ? "dark" : "light") : theme;
  return themePresets.find((preset) => preset.id === id) ?? themePresets[0];
}

function channels(color: string): number[] {
  return [1, 3, 5].map((index) => Number.parseInt(color.slice(index, index + 2), 16));
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

export function paletteVariables(preset: ThemePreset): Record<string, string> {
  const p = preset.palette;
  const dark = preset.scheme === "dark";
  const colors = ["#fafafa", "#f4f4f5", "#e4e4e7", "#d4d4d8", "#a1a1aa", "#71717a", "#52525b", "#3f3f46", "#27272a", "#18181b", "#09090b"];
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

function blurFilter(pixels: number): string {
  return pixels === 0 ? "none" : `blur(${pixels}px)`;
}

export function applyAppearance(
  root: HTMLElement, theme: Theme, appearance: Appearance, prefersDark: boolean, hyprland: boolean,
): void {
  const palette = activePalette(theme, prefersDark);
  const windowTransparent = appearance.transparent && hyprland;
  root.dataset.theme = palette.scheme;
  root.dataset.palette = theme;
  root.dataset.transparent = String(windowTransparent);
  root.style.colorScheme = palette.scheme;
  for (const [key, value] of Object.entries(paletteVariables(palette))) root.style.setProperty(key, value);
  root.style.setProperty("--legio-image-blur", appearance.backgroundBlur + "px");
  root.style.setProperty("--legio-image-opacity", String(appearance.backgroundOpacity / 100));
  root.style.setProperty("--legio-surface-opacity", String(appearance.surfaceOpacity / 100));
  root.style.setProperty("--legio-surface-blur", blurFilter(appearance.surfaceBlur));
  root.style.setProperty("--legio-dialog-opacity", String(appearance.dialogOpacity / 100));
  root.style.setProperty("--legio-dialog-blur", blurFilter(appearance.dialogBlur));
}

export async function chooseBackground(): Promise<string | null> {
  const path = await open({
    title: t("Scegli uno sfondo"), multiple: false,
    filters: [{ name: t("Immagini"), extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
  return path === null ? null : invoke<string>("import_theme_background", { source: path });
}

export function cleanupBackgrounds(): Promise<void> {
  return invoke("cleanup_theme_backgrounds");
}

export async function loadBackground(id: string): Promise<string> {
  const bytes = await invoke<ArrayBuffer>("get_theme_background", { id });
  return URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
}
