import { t } from "../i18n";
import { invoke } from "@tauri-apps/api/core";

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
}

export interface InstalledFolderInfo {
  directory: string;
  freeBytes: number;
  totalBytes: number;
}

export interface StagedExecutableCandidate {
  relativePath: string;
  score: number;
  signals: string[];
}

export interface StagedExecutableScan {
  candidates: StagedExecutableCandidate[];
  selectedRelativePath: string | null;
}

export type DownloadStatus = "queued" | "downloading" | "waiting" | "paused" | "failed" | "downloaded" | "staging" | "staged" | "finalizing" | "installed" | "cancelled";

const statusLabels: Record<DownloadStatus, string> = {
  queued: "in coda",
  downloading: "download in corso",
  waiting: "in attesa",
  paused: "download in pausa",
  failed: "download fallito",
  downloaded: "download completato",
  staging: "estrazione in corso",
  staged: "estrazione completata",
  finalizing: "installazione in corso",
  installed: "installato",
  cancelled: "download annullato",
};

export function describeDownloadStatus(status: string): string {
  return t(statusLabels[status as DownloadStatus] ?? status);
}

export function isActiveDownloadStatus(status: string): boolean {
  return status === "queued" || status === "downloading" || status === "waiting" || status === "downloaded" || status === "staging" || status === "finalizing";
}

export function canPauseDownload(status: string): boolean {
  return status === "queued" || status === "downloading" || status === "waiting";
}

export function canResumeDownload(status: string): boolean {
  return status === "paused" || status === "waiting";
}

export function canRetryDownload(status: string, error?: string | null): boolean {
  return status === "failed" || (status === "finalizing" && Boolean(error));
}

export function canCancelDownload(status: string): boolean {
  return (
    status === "queued" ||
    status === "downloading" ||
    status === "paused" ||
    status === "waiting" ||
    status === "failed"
  );
}

export function canFinalizeDownload(status: string): boolean {
  return status === "staged";
}

export function canRemoveDownload(status: string): boolean {
  return status === "cancelled" || status === "failed" || status === "installed";
}

export function isFinishedDownloadStatus(status: string): boolean {
  return status === "cancelled" || status === "installed";
}

export function downloadProgressPercent(job: DownloadJob): number {
  if (job.sizeBytes <= 0) return 0;
  return Math.min(100, (job.downloadedBytes / job.sizeBytes) * 100);
}

export function listDownloads(): Promise<DownloadJob[]> {
  return invoke<DownloadJob[]>("list_downloads");
}

export function queueDownload(steamAppId: number, downloadUrl: string, releaseVersion: string, acceptUnverified: boolean): Promise<DownloadJob> {
  return invoke<DownloadJob>("queue_download", { steamAppId, downloadUrl, releaseVersion, acceptUnverified });
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

export function scanStagedExecutables(
  id: string,
  gameName: string | null,
): Promise<StagedExecutableScan> {
  return invoke<StagedExecutableScan>("scan_staged_executables", { id, gameName });
}

export function finalizeDownload(id: string, executableRelative: string): Promise<void> {
  return invoke<void>("finalize_download", { id, executableRelative });
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
