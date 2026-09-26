import { invoke } from "@tauri-apps/api/core";

export interface DownloadJob {
  id: string;
  steamAppId: number;
  name: string;
  releaseVersion: string;
  sizeBytes: number;
  downloadedBytes: number;
  speedBps: number;
  etaSeconds: number | null;
  status: string;
  error: string | null;
}

const statusLabels: Record<string, string> = {
  queued: "In coda",
  downloading: "Download in corso",
  waiting: "In attesa",
  paused: "In pausa",
  failed: "Fallito",
  downloaded: "Scaricato",
  staging: "Estrazione",
  staged: "Pronto",
  finalizing: "Installazione",
  installed: "Installato",
  cancelled: "Annullato",
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

export function listDownloads(): Promise<DownloadJob[]> {
  return invoke<DownloadJob[]>("list_downloads");
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
