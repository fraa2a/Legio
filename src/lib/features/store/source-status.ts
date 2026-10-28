import type { CatalogGame } from "../../services/catalog";
import type { InstalledSource, SourceEntry, SourceManifest } from "../../services/legio-source";

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

const releaseIndexes = new WeakMap<SourceManifest, Map<number, SourceReleaseStatus[]>>();

function releaseIndex(manifest: SourceManifest): Map<number, SourceReleaseStatus[]> {
  const cached = releaseIndexes.get(manifest);
  if (cached !== undefined) return cached;
  const index = new Map<number, SourceReleaseStatus[]>();
  for (const availability of ["verified", "unverified"] as const) {
    for (const entry of manifest[availability]) {
      const releases = index.get(entry.steamAppId);
      if (releases === undefined) index.set(entry.steamAppId, [{ availability, entry }]);
      else releases.push({ availability, entry });
    }
  }
  releaseIndexes.set(manifest, index);
  return index;
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
    tone: "neutral",
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
  return releaseIndex(manifest).get(steamAppId)?.[0] ?? { availability: "unavailable", entry: null };
}

export function sourceReleasesFor(
  manifest: SourceManifest | null,
  steamAppId: number,
): SourceReleaseStatus[] {
  if (manifest === null) return [];
  return releaseIndex(manifest).get(steamAppId) ?? [];
}

export function sourceReleaseId(entry: SourceEntry): string {
  return entry.sourceId ? JSON.stringify([entry.sourceId, entry.id ?? entry.download.url, entry.release.version]) : JSON.stringify([entry.download.url, entry.release.version]);
}

export function sourceNameFor(entry: SourceEntry, sources: InstalledSource[]): string | undefined {
  const origin = sources.find(source => source.id === entry.sourceId);
  if (origin === undefined) return undefined;
  return origin.name || new URL(origin.url).hostname;
}

export function releaseLinks(entry: SourceEntry, sources: InstalledSource[]): string[] {
  return sources.find(origin => origin.id === entry.sourceId)?.candidates?.[entry.id ?? ""] ?? [entry.download.url];
}

export function reachableReleases(
  releases: SourceReleaseStatus[],
  sources: InstalledSource[],
  sizes: Record<string, number>,
): SourceReleaseStatus[] {
  return releases.filter(release => releaseLinks(release.entry, sources).some(uri => uri in sizes));
}
