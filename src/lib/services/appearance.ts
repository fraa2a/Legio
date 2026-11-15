import { invoke } from "./invoke";
import { open, save } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";
import presets from "./theme-presets.json";
import { luminance, tintColor, type Tint } from "./color-tint";
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
  tint?: Tint;
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
  accent: string | null;
  background: string | null;
  backgroundBlur: number;
  backgroundOpacity: number;
  animatedBackground: AnimatedBackground;
  animatedOpacity: number;
  dither: DitherSettings;
  season: Season;
  starDensity: StarDensity;
  // Noise overlay over any background, 0 to 100, 0 is off.
  grain: number;
  surfaceOpacity: number;
  surfaceBlur: number;
  dialogOpacity: number;
  dialogBlur: number;
  transparent: boolean;
  density: Density;
  corners: Corners;
  textScale: number;
  gameAccent: boolean;
}

export type AnimatedBackground = "none" | "particles" | "dither" | "scanlines" | "aurora" | "ambient" | "starfield" | "seasonal";
export type Season = "snow" | "rain" | "leaves";
export type StarDensity = "low" | "medium" | "high";
export type Density = "compact" | "normal" | "wide";
export type Corners = "sharp" | "soft" | "round";

export const textScaleRange = { min: 90, max: 120 } as const;

// Multipliers on the control heights and on container padding and rhythm.
export const densityScales: Record<Density, { control: number; space: number }> = {
  compact: { control: 0.875, space: 0.75 },
  normal: { control: 1, space: 1 },
  wide: { control: 1.125, space: 1.25 },
};

// Multiplier on the control, card and panel radii.
export const cornerScales: Record<Corners, number> = { sharp: 0.25, soft: 1, round: 1.75 };

export const themePresets = presets as ThemePreset[];

export type PanelStyle = "flat" | "glass" | "solid";
type PanelValues = Pick<Appearance, "surfaceOpacity" | "surfaceBlur" | "dialogOpacity" | "dialogBlur">;

export const panelStyles: Record<PanelStyle, PanelValues> = {
  flat: { surfaceOpacity: 0, surfaceBlur: 40, dialogOpacity: 0, dialogBlur: 40 },
  glass: { surfaceOpacity: 45, surfaceBlur: 24, dialogOpacity: 45, dialogBlur: 35 },
  solid: { surfaceOpacity: 90, surfaceBlur: 0, dialogOpacity: 100, dialogBlur: 0 },
};

// Null when the sliders were tuned to values that match no preset.
export function panelStyleOf(appearance: Appearance): PanelStyle | null {
  const styles = Object.keys(panelStyles) as PanelStyle[];
  return styles.find((style) => (Object.keys(panelStyles[style]) as (keyof PanelValues)[]).every((key) => appearance[key] === panelStyles[style][key])) ?? null;
}

export function defaultDither(): DitherSettings {
  return {
    waveSpeed: 0.05, waveFrequency: 3, waveAmplitude: 0.3,
    waveColor: null, backgroundColor: "#000000", colorNum: 4, pixelSize: 2,
    disableAnimation: false, enableMouseInteraction: true, mouseRadius: 1,
  };
}

export function defaultAppearance(): Appearance {
  return {
    accent: null, background: null, backgroundBlur: 0, backgroundOpacity: 60,
    animatedBackground: "none", animatedOpacity: 65, dither: defaultDither(),
    season: "snow", starDensity: "medium", grain: 0,
    ...panelStyles.glass, transparent: false,
    density: "normal", corners: "soft", textScale: 100, gameAccent: false,
  };
}

export function activePalette(theme: Theme, prefersDark: boolean): ThemePreset {
  const id = theme === "system" ? (prefersDark ? "dark" : "light") : theme;
  return themePresets.find((preset) => preset.id === id) ?? themePresets[0];
}

export function accentText(color: string): string {
  const value = luminance(color);
  return (value + 0.05) / 0.05 >= 1.05 / (value + 0.05) ? "#000000" : "#ffffff";
}

const zincShades = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];
const zincRamp = ["#fafafa", "#f4f4f5", "#e4e4e7", "#d4d4d8", "#a1a1aa", "#71717a", "#52525b", "#3f3f46", "#27272a", "#18181b", "#09090b"];

export function resolvedAccent(preset: ThemePreset, appearance: Appearance): string {
  return appearance.accent ?? preset.palette.accent;
}

// Every neutral in the UI is a zinc shade, so a tinted palette recolors that ramp.
export function paletteVariables(preset: ThemePreset, accent: string = preset.palette.accent): Record<string, string> {
  const p = preset.palette;
  const dark = preset.scheme === "dark";
  const ramp = preset.tint === undefined ? zincRamp : zincRamp.map((color) => tintColor(color, preset.tint as Tint));
  const variables: Record<string, string> = {
    "--legio-background": p.background, "--legio-surface": p.surface,
    "--legio-text": p.text, "--legio-muted": p.muted,
    "--legio-accent": accent, "--legio-accent-text": accentText(accent),
    "--legio-focus": accent,
    "--color-white": dark ? p.text : "#ffffff",
  };
  zincShades.forEach((shade, index) => {
    variables["--color-zinc-" + shade] = ramp[index];
  });
  variables["--color-black"] = dark ? p.background : "#000000";
  return variables;
}

// The scale tokens in app.css multiply these variables. Unknown values fall back to the defaults.
export function interfaceVariables(appearance: Pick<Appearance, "density" | "corners" | "textScale">): Record<string, string> {
  const density = densityScales[appearance.density] ?? densityScales.normal;
  const textScale = Math.min(textScaleRange.max, Math.max(textScaleRange.min, appearance.textScale || 100));
  return {
    "--legio-control-scale": String(density.control),
    "--legio-space-scale": String(density.space),
    "--legio-corner-scale": String(cornerScales[appearance.corners] ?? cornerScales.soft),
    "--legio-text-scale": String(textScale / 100),
  };
}

function blurFilter(pixels: number): string {
  return pixels === 0 ? "none" : `blur(${pixels}px)`;
}

// A per-game accent replaces the resolved one; its color is already adjusted for the palette.
export function applyAppearance(
  root: HTMLElement, theme: Theme, appearance: Appearance, prefersDark: boolean, hyprland: boolean,
  accentOverride: string | null = null,
): void {
  const palette = activePalette(theme, prefersDark);
  const windowTransparent = appearance.transparent && hyprland;
  root.dataset.theme = palette.scheme;
  root.dataset.palette = theme;
  root.dataset.transparent = String(windowTransparent);
  root.dataset.accentFade = String(appearance.gameAccent);
  root.style.colorScheme = palette.scheme;
  for (const [key, value] of Object.entries(paletteVariables(palette, accentOverride ?? resolvedAccent(palette, appearance)))) root.style.setProperty(key, value);
  for (const [key, value] of Object.entries(interfaceVariables(appearance))) root.style.setProperty(key, value);
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

const profileFilters = () => [{ name: "JSON", extensions: ["json"] }];

export interface AppearanceProfile {
  version: number;
  theme: Theme;
  appearance: Appearance;
}

export async function exportAppearance(theme: Theme, appearance: Appearance): Promise<boolean> {
  const path = await save({ title: t("Esporta aspetto"), defaultPath: "legio-appearance.json", filters: profileFilters() });
  if (path === null) return false;
  await invoke("export_appearance", { path, profile: { version: 1, theme, appearance } });
  return true;
}

export async function importAppearance(): Promise<AppearanceProfile | null> {
  const path = await open({ title: t("Importa aspetto"), multiple: false, filters: profileFilters() });
  return path === null ? null : invoke<AppearanceProfile>("import_appearance", { path });
}

export function createThemeCode(theme: Theme, appearance: Appearance): Promise<string> {
  return invoke<string>("create_theme_code", { profile: { version: 1, theme, appearance } });
}

export function readThemeCode(code: string): Promise<AppearanceProfile> {
  return invoke<AppearanceProfile>("read_theme_code", { code });
}

export function cleanupBackgrounds(): Promise<void> {
  return invoke("cleanup_theme_backgrounds");
}

export async function loadBackground(id: string): Promise<string> {
  const bytes = await invoke<ArrayBuffer>("get_theme_background", { id });
  return URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
}
