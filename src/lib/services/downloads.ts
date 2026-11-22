import { downloadState, type DownloadStatus } from "./download-status";
import { t } from "../i18n";
import { invoke } from "./invoke";

/** Commands the backend accepts for the job in its current state. */
export interface DownloadActions {
  pause: boolean;
  resume: boolean;
  retry: boolean;
  cancel: boolean;
  remove: boolean;
  reorder: boolean;
}

export interface DownloadJob {
  id: string;
  steamAppId: number;
  name: string;
  releaseVersion: string;
  sha256: string | null;
  url: string;
  sizeBytes: number;
  downloadedBytes: number;
  speedBps: number;
  etaSeconds: number | null;
  status: DownloadStatus;
  error: string | null;
  updatedAt: number;
  targetGameId?: string | null;
  pauseReason?: "game_running" | "playing" | "automatic_disabled" | null;
  automaticUpdate?: boolean;
  sourceId?: string | null;
  sourceEntryId?: string | null;
  /** Part being written and part count of a multipart archive. */
  parts?: { current: number; total: number } | null;
  actions: DownloadActions;
}

export type SpaceVerdict = "enough" | "tight" | "insufficient" | "unknown";

export interface VolumeSpace {
  path: string;
  freeBytes: number | null;
  reservedBytes: number;
  minimumBytes: number;
  estimatedBytes: number;
}

export interface SpaceForecast {
  verdict: SpaceVerdict;
  sizeBytes: number | null;
  volumes: VolumeSpace[];
}

export interface InstalledFolderInfo {
  directory: string;
  freeBytes: number;
  totalBytes: number;
}

export type { DownloadStatus } from "./download-status";

export function describeDownloadStatus(status: string): string {
  return t(downloadState(status)?.description ?? status);
}

export function isActiveDownloadStatus(status: string): boolean {
  return downloadState(status)?.active ?? false;
}

export function isFinishedDownloadStatus(status: string): boolean {
  return downloadState(status)?.section === "finished";
}

// Mirrors has_unfinished_work in download_queue.rs, which decides whether shutdown can be armed.
export function hasUnfinishedDownloadWork(jobs: readonly DownloadJob[]): boolean {
  return jobs.some((job) =>
    ["queued", "downloading", "waiting"].includes(job.status)
    || (job.status === "paused" && job.pauseReason !== "automatic_disabled")
    || (["downloaded", "staging", "staged", "finalizing"].includes(job.status) && job.error === null));
}

export function downloadProgressPercent(job: DownloadJob): number {
  if (!Number.isFinite(job.sizeBytes) || !Number.isFinite(job.downloadedBytes) || job.sizeBytes <= 0) return 0;
  return Math.min(100, Math.max(0, (job.downloadedBytes / job.sizeBytes) * 100));
}

export function listDownloads(): Promise<DownloadJob[]> {
  return invoke<DownloadJob[]>("list_downloads");
}

export function queueDownload(steamAppId: number, downloadUrl: string, releaseVersion: string, acceptUnverified: boolean, sourceId?: string | null, preferredUri?: string | null, libraryId?: string | null): Promise<DownloadJob> {
  return invoke<DownloadJob>("queue_download", { steamAppId, downloadUrl, releaseVersion, acceptUnverified, ...(sourceId ? { sourceId } : {}), ...(preferredUri ? { preferredUri } : {}), ...(libraryId ? { libraryId } : {}) });
}

export function forecastDownloadSpace(sizeBytes: number | null, libraryId: string | null): Promise<SpaceForecast> {
  return invoke<SpaceForecast>("forecast_download_space", { sizeBytes, libraryId });
}

export function pauseDownload(id: string): Promise<void> {
  return invoke<void>("pause_download", { id });
}

export function resumeDownload(id: string): Promise<void> {
  return invoke<void>("resume_download", { id });
}

export function retryDownload(id: string): Promise<void> {
  return invoke<void>("retry_download", { id });
}

export function cancelDownload(id: string): Promise<void> {
  return invoke<void>("cancel_download", { id });
}

export function removeDownload(id: string): Promise<void> {
  return invoke<void>("remove_download", { id });
}

export function removeFinishedDownloads(): Promise<string[]> {
  return invoke<string[]>("remove_finished_downloads");
}

export function reorderDownloads(ids: string[]): Promise<void> {
  return invoke<void>("reorder_downloads", { ids });
}



export function getDownloadBandwidthLimit(): Promise<number> {
  return invoke<number>("get_download_bandwidth_limit");
}

export function setDownloadBandwidthLimit(bytesPerSecond: number): Promise<void> {
  return invoke<void>("set_download_bandwidth_limit", { bytesPerSecond });
}

export function openInstalledFolder(): Promise<void> {
  return invoke<void>("open_installed_folder");
}

export function getInstalledFolderInfo(): Promise<InstalledFolderInfo> {
  return invoke<InstalledFolderInfo>("get_installed_folder_info");
}
