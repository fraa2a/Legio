import { invoke } from "@tauri-apps/api/core";

export interface DownloadJob {
  id: string;
  steamAppId: number;
  name: string;
  releaseVersion: string;
  sha256: string;
  sizeBytes: number;
  downloadedBytes: number;
  speedBps: number;
  etaSeconds: number | null;
  status: string;
  error: string | null;
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

const statusLabels: Record<string, string> = {
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
  return statusLabels[status] ?? status;
}

export function isActiveDownloadStatus(status: string): boolean {
  return status === "queued" || status === "downloading" || status === "waiting" || status === "staging" || status === "finalizing";
}

export function canPauseDownload(status: string): boolean {
  return status === "queued" || status === "downloading" || status === "waiting";
}

export function canResumeDownload(status: string): boolean {
  return status === "paused" || status === "waiting";
}

export function canRetryDownload(status: string): boolean {
  return status === "failed";
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

export function canStageDownload(status: string): boolean {
  return status === "downloaded";
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

export function queueDownload(steamAppId: number, sha256: string, acceptUnverified: boolean): Promise<DownloadJob> {
  return invoke<DownloadJob>("queue_download", { steamAppId, sha256, acceptUnverified });
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

export function stageDownload(id: string): Promise<string> {
  return invoke<string>("stage_download", { id });
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
