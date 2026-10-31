import { t, type Language } from "../../i18n";
import type { ModInstallProgress, ProfileImportOutcome } from "../../services/mod-profile-exchange";

export const maxProfileNameLength = 64;
const profileCodePattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const controlCharacters = /[\u0000-\u001f\u007f]/;
const invalidFileNameCharacters = /[<>:"/\\|?*\u0000-\u001f]/g;

export type ImportSource = "file" | "code";

export function normalizeProfileCode(input: string): string | null {
  const code = input.trim();
  return profileCodePattern.test(code) ? code.toLowerCase() : null;
}

export function profileCodeError(input: string, language?: Language): string | null {
  if (input.trim().length === 0) return null;
  return normalizeProfileCode(input) === null ? t("Il codice deve avere il formato 123e4567-e89b-12d3-a456-426614174000", language) : null;
}

export function profileNameError(input: string, language?: Language): string | null {
  const name = input.trim();
  if (name.length === 0) return null;
  if (name.length > maxProfileNameLength) return t("Il nome può avere al massimo {0} caratteri", language, [maxProfileNameLength]);
  if (controlCharacters.test(name) || /[/\\]/.test(name)) return t("Il nome non può contenere barre o caratteri di controllo", language);
  return null;
}

export function canStartImport(source: ImportSource, path: string | null, code: string, name: string): boolean {
  if (profileNameError(name) !== null) return false;
  return source === "file" ? path !== null : normalizeProfileCode(code) !== null;
}

export function fileNameOf(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

export function exportFileName(profileName: string): string {
  const stem = profileName.replace(invalidFileNameCharacters, "-").trim().replace(/[. ]+$/, "");
  return `${stem.length > 0 ? stem : "profile"}.r2z`;
}

export interface ProgressView {
  label: string;
  percent: number | null;
}

export function describeProgress(progress: ModInstallProgress | null, language?: Language): ProgressView {
  if (progress === null || progress.stage === "resolving") {
    return { label: t("Risoluzione delle dipendenze...", language), percent: null };
  }
  if (progress.stage === "done") return { label: t("Completato", language), percent: 100 };
  const name = progress.package ?? "";
  const position = progress.total > 0 ? [progress.current, progress.total] : null;
  const label = progress.stage === "downloading" ? t("Download di {0}", language, [name])
    : progress.stage === "extracting" ? t("Estrazione di {0}", language, [name])
    : t("Installazione di {0}", language, [name]);
  if (position === null) return { label, percent: null };
  const fraction = progress.stage === "downloading" && progress.totalBytes !== null && progress.totalBytes > 0 && progress.downloadedBytes !== null
    ? Math.min(1, progress.downloadedBytes / progress.totalBytes)
    : progress.stage === "downloading" ? 0 : 1;
  const percent = Math.min(100, Math.round(((position[0] - 1 + fraction) / position[1]) * 100));
  return { label: `${label} (${position[0]}/${position[1]})`, percent };
}

export interface ImportSummary {
  installed: number;
  skippedFiles: string[];
  missingPackages: string[];
  warnings: string[];
  clean: boolean;
}

export function summarizeImport(outcome: ProfileImportOutcome): ImportSummary {
  const { installed, skippedFiles, missingPackages, warnings } = outcome;
  return {
    installed: installed.length,
    skippedFiles,
    missingPackages,
    warnings,
    clean: skippedFiles.length === 0 && missingPackages.length === 0 && warnings.length === 0,
  };
}
