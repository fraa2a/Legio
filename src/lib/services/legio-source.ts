import { invoke } from "@tauri-apps/api/core";

export interface SourceEntry {
  steamAppId: number;
  name: string;
  release: { version: string; publishedAt: string };
  download: { url: string; sha256?: string | null; sizeBytes: number };
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
  sources: InstalledSource[];
}

export function getLegioSource(): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("get_legio_source");
}

export function refreshLegioSource(): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("refresh_legio_source");
}

export interface InstalledSource {
  id: string;
  url: string;
  cachedAt: number;
  stale: boolean;
  warning: string | null;
  gameCount: number;
}

export function addDownloadSource(url: string): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("add_download_source", { url });
}

export function removeDownloadSource(id: string): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("remove_download_source", { id });
}

export interface PendingSourceLinks {
  requests: { url: string | null; error: string | null }[];
  registrationError: string | null;
}

export function takeSourceLinks(): Promise<PendingSourceLinks> {
  return invoke<PendingSourceLinks>("take_source_links");
}
