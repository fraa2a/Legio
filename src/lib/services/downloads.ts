import { downloadState, supportsDownloadAction, type DownloadStatus } from "./download-status";
import { t } from "../i18n";
import { invoke } from "./invoke";

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

export function canPauseDownload(status: string): boolean {
  return supportsDownloadAction(status, "pause");
}

export function canResumeDownload(status: string): boolean {
  return supportsDownloadAction(status, "resume");
}

export function canRetryDownload(status: string, error?: string | null): boolean {
  return supportsDownloadAction(status, "retry") || ((status === "finalizing" || status === "staged") && Boolean(error));
}

export function canCancelDownload(status: string): boolean {
  return supportsDownloadAction(status, "cancel");
}

export function canFinalizeDownload(status: string): boolean {
  return supportsDownloadAction(status, "finalize");
}

export function canRemoveDownload(status: string): boolean {
  return supportsDownloadAction(status, "remove");
}

export function isFinishedDownloadStatus(status: string): boolean {
  return downloadState(status)?.section === "finished";
}

export function downloadProgressPercent(job: DownloadJob): number {
  if (job.sizeBytes <= 0) return 0;
  return Math.min(100, (job.downloadedBytes / job.sizeBytes) * 100);
}

export function listDownloads(): Promise<DownloadJob[]> {
  return invoke<DownloadJob[]>("list_downloads");
}

export function queueDownload(steamAppId: number, downloadUrl: string, releaseVersion: string, acceptUnverified: boolean, sourceId?: string | null, preferredUri?: string | null): Promise<DownloadJob> {
  return invoke<DownloadJob>("queue_download", { steamAppId, downloadUrl, releaseVersion, acceptUnverified, ...(sourceId ? { sourceId } : {}), ...(preferredUri ? { preferredUri } : {}) });
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
