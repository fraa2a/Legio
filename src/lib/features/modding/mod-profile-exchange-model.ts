import { t, type Language } from "../../i18n";
import type { ProfileImportOutcome } from "../../services/mod-profile-exchange";
import type { ModProfile } from "../../services/modding";

const profileCodePattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const invalidFileNameCharacters = /[<>:"/\\|?*\u0000-\u001f]/g;

export type ImportSource = "file" | "code";
export type ImportTarget = "new" | "existing";

export function normalizeProfileCode(input: string): string | null {
  const code = input.trim();
  return profileCodePattern.test(code) ? code.toLowerCase() : null;
}

export function profileCodeError(input: string, language?: Language): string | null {
  if (input.trim().length === 0) return null;
  return normalizeProfileCode(input) === null ? t("Il codice deve avere il formato 123e4567-e89b-12d3-a456-426614174000", language) : null;
}

export function canStartImport(
  source: ImportSource,
  path: string | null,
  code: string,
  nameError: string | null,
  target: ImportTarget = "new",
  targetProfileId: string | null = null,
): boolean {
  if (target === "existing" ? targetProfileId === null : nameError !== null) return false;
  return source === "file" ? path !== null : normalizeProfileCode(code) !== null;
}

// The active profile is the likeliest one to refresh; without it the first profile.
export function defaultTargetProfileId(profiles: readonly ModProfile[], activeProfileId: string | null): string | null {
  return profiles.find((profile) => profile.id === activeProfileId)?.id ?? profiles[0]?.id ?? null;
}

// Only an update names a target; a new profile leaves it out so the backend creates one.
export function importTargetProfileId(target: ImportTarget, targetProfileId: string | null): string | undefined {
  return target === "existing" ? targetProfileId ?? undefined : undefined;
}

export function fileNameOf(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

export function exportFileName(profileName: string): string {
  const stem = profileName.replace(invalidFileNameCharacters, "-").trim().replace(/[. ]+$/, "");
  return `${stem.length > 0 ? stem : "profile"}.r2z`;
}

export interface ImportSummary {
  installed: number;
  skippedFiles: string[];
  missingPackages: string[];
  warnings: string[];
}

export function summarizeImport(outcome: ProfileImportOutcome): ImportSummary {
  const { installed, skippedFiles, missingPackages, warnings } = outcome;
  return {
    installed: installed.length,
    skippedFiles,
    missingPackages,
    warnings,
  };
}
