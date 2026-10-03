import { t } from "../../i18n";
import type { CatalogGame } from "../../services/catalog";
import type { SourceEntry, SourceManifest } from "../../services/legio-source";

export type SourceAvailability = CatalogGame["availability"];

export type SourceTone = "neutral" | "success" | "warning" | "danger";

export interface SourceStatus {
  availability: SourceAvailability;
  entry: SourceEntry | null;
}

export interface SourceReleaseStatus {
  availability: "verified" | "unverified";
  entry: SourceEntry;
}

export const availabilityMeta: Record<
  SourceAvailability,
  { label: string; tone: SourceTone; description: string }
> = {
  verified: {
    label: t("Verificato", undefined),
    tone: "success",
    description: "",
  },
  unverified: {
    label: t("Non verificato", undefined),
    tone: "warning",
    description:
      "",
  },
  unavailable: {
    label: t("Non disponibile", undefined),
    tone: "danger",
    description: "",
  },
  unknown: {
    label: "Sconosciuto",
    tone: "neutral",
    description:
      "",
  },
};

export function sourceStatusFor(
  manifest: SourceManifest | null,
  steamAppId: number,
): SourceStatus {
  if (manifest === null) return { availability: "unknown", entry: null };
  const verified = manifest.verified.find((entry) => entry.steamAppId === steamAppId);
  if (verified !== undefined) return { availability: "verified", entry: verified };
  const unverified = manifest.unverified.find((entry) => entry.steamAppId === steamAppId);
  if (unverified !== undefined) return { availability: "unverified", entry: unverified };
  return { availability: "unavailable", entry: null };
}

export function sourceReleasesFor(
  manifest: SourceManifest | null,
  steamAppId: number,
): SourceReleaseStatus[] {
  if (manifest === null) return [];
  return [
    ...manifest.verified.filter((entry) => entry.steamAppId === steamAppId).map((entry) => ({ availability: "verified" as const, entry })),
    ...manifest.unverified.filter((entry) => entry.steamAppId === steamAppId).map((entry) => ({ availability: "unverified" as const, entry })),
  ];
}

export function sourceReleaseId(entry: SourceEntry): string {
  return JSON.stringify([entry.download.url, entry.release.version]);
}
