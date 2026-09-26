import type { CatalogGame } from "../../services/catalog";
import type { SourceEntry, SourceManifest } from "../../services/legio-source";

export type SourceAvailability = CatalogGame["availability"];

export type SourceTone = "neutral" | "success" | "warning" | "danger";

export interface SourceStatus {
  availability: SourceAvailability;
  entry: SourceEntry | null;
}

export const availabilityMeta: Record<
  SourceAvailability,
  { label: string; tone: SourceTone; description: string }
> = {
  verified: {
    label: "Verificato",
    tone: "success",
    description: "",
  },
  unverified: {
    label: "Non verificato",
    tone: "warning",
    description:
      "",
  },
  unavailable: {
    label: "Non disponibile",
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
