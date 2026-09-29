import { invoke } from "@tauri-apps/api/core";

export interface SourceEntry {
  steamAppId: number;
  name: string;
  release: { version: string; publishedAt: string };
  download: { url: string; sha256: string; sizeBytes: number };
}

export interface SourceManifest {
  schemaVersion: number;
  generatedAt: string;
  verified: SourceEntry[];
  unverified: SourceEntry[];
}

export interface SourceSnapshot {
  manifest: SourceManifest | null;
  cachedAt: number | null;
  stale: boolean;
  warning: string | null;
}

export function getLegioSource(): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("get_legio_source");
}

export function refreshLegioSource(): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("refresh_legio_source");
}
