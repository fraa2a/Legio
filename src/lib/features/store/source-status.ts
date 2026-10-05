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
  { label: string; tone: SourceTone }
> = {
  verified: {
    label: "Verificato",
    tone: "success",
  },
  unverified: {
    label: "Non verificato",
    tone: "warning",
  },
  unavailable: {
    label: "Non disponibile",
    tone: "danger",
  },
  unknown: {
    label: "Sconosciuto",
    tone: "neutral",
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
