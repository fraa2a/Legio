import { t, type Language } from "../../i18n";
import { type DownloadJob } from "../../services/downloads";

import { downloadState, type StatusTone, type ProgressTone } from "../../services/download-status";
export type { StatusTone } from "../../services/download-status";

export interface DownloadSections {
  primary: DownloadJob | null;
  processing: DownloadJob[];
  queue: DownloadJob[];
  finished: DownloadJob[];
}

function bucket(job: DownloadJob): "processing" | "queue" | "finished" {
  return downloadState(job.status)?.section ?? "queue";
}

export function splitDownloads(jobs: DownloadJob[]): DownloadSections {
  const sections: Record<ReturnType<typeof bucket>, DownloadJob[]> = { processing: [], queue: [], finished: [] };
  for (const job of jobs) sections[bucket(job)].push(job);
  const { processing, queue, finished } = sections;
  const primary =
    processing[0] ??
    queue.find((job) => job.status === "paused") ??
    queue.find((job) => job.status === "queued" || job.status === "waiting") ??
    null;
  return {
    primary,
    processing: processing.filter((job) => job !== primary),
    queue: queue.filter((job) => job !== primary),
    finished,
  };
}

export function isReorderable(job: DownloadJob): boolean {
  return downloadState(job.status)?.reorderable ?? false;
}

export function reorderPayload(primary: DownloadJob | null, queue: DownloadJob[]): string[] {
  return [primary, ...queue].filter((job): job is DownloadJob => job !== null && isReorderable(job)).map((job) => job.id);
}

export function statusTone(status: string): StatusTone {
  return downloadState(status)?.tone ?? "neutral";
}

export function progressTone(status: DownloadJob["status"]): ProgressTone {
  return downloadState(status)?.progress ?? "default";
}

export function statusBadge(status: string, selected?: Language): string {
  return t(downloadState(status)?.badge ?? status, selected);
}

export function phaseLabel(job: DownloadJob, selected?: Language): string | null {
  if (job.pauseReason === "game_running") return t("Gioco aperto: aggiornamento bloccato", selected);
  if (job.pauseReason === "playing") return t("In pausa mentre giochi", selected);
  if (job.pauseReason === "automatic_disabled") return t("Aggiornamenti automatici disattivati", selected);
  if (job.status === "downloading") return `Download ${Math.round(progressOf(job))}%`;
  const phase = downloadState(job.status)?.phase;
  return phase === null ? null : t(phase ?? job.status, selected);
}

export function progressOf(job: DownloadJob): number {
  if (job.sizeBytes <= 0) return 0;
  return Math.min(100, (job.downloadedBytes / job.sizeBytes) * 100);
}

export function formatEta(seconds: number | null): string {
  if (seconds === null) return "-";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function downloadBytes(jobs: DownloadJob[]): number {
  return jobs.reduce((total, job) => total + job.sizeBytes, 0);
}

export function activeSpeed(jobs: DownloadJob[]): number {
  return jobs.reduce((total, job) => total + (job.status === "downloading" ? job.speedBps : 0), 0);
}
