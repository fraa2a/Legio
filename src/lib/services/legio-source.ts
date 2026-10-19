import { listen } from "@tauri-apps/api/event";
import { invoke } from "./invoke";

export interface SourceEntry {
  id?: string | null;
  sourceId?: string | null;
  steamAppId: number;
  name: string;
  release: { version: string; publishedAt: string | null };
  download: { url: string; sha256?: string | null; sizeBytes: number | null };
}

export interface SourceManifest {
  schemaVersion: number;
  supportGameUpdates?: boolean;
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
  isLocal?: boolean;
  cachedAt: number;
  stale: boolean;
  warning: string | null;
  gameCount: number;
  supportGameUpdates?: boolean;
  name?: string;
  format?: "legio" | "hydra";
  diagnostics?: { rejected: number; unsupported: number; unresolved: number; reasons: string[] };
  candidates?: Record<string, string[]>;
}

export function addDownloadSource(url: string): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("add_download_source", { url });
}

export function addLocalDownloadSource(path: string): Promise<SourceSnapshot> {
  return invoke<SourceSnapshot>("add_local_download_source", { path });
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

export interface SourceImportProgress { operationId: string; processed: number; total: number }
export function observeSourceImportProgress(callback: (progress: SourceImportProgress) => void): Promise<() => void> {
  return listen<SourceImportProgress>("source-import-progress", event => callback(event.payload));
}
