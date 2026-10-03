import { t, type Language } from "../../i18n";
import { type DownloadJob } from "../../services/downloads";

const PROCESSING = ["downloading", "downloaded", "staging", "staged", "finalizing"];
const FINISHED = ["installed", "cancelled"];
const REORDERABLE = ["queued", "paused", "waiting", "failed"];

export type StatusTone = "neutral" | "info" | "success" | "danger" | "warning";

export interface DownloadSections {
  primary: DownloadJob | null;
  processing: DownloadJob[];
  queue: DownloadJob[];
  finished: DownloadJob[];
}

function bucket(job: DownloadJob): "processing" | "queue" | "finished" {
  if (PROCESSING.includes(job.status)) return "processing";
  if (FINISHED.includes(job.status)) return "finished";
  return "queue";
}

export function splitDownloads(jobs: DownloadJob[]): DownloadSections {
  const processing = jobs.filter((job) => bucket(job) === "processing");
  const queue = jobs.filter((job) => bucket(job) === "queue");
  const finished = jobs.filter((job) => bucket(job) === "finished");
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
  return REORDERABLE.includes(job.status);
}

export function reorderPayload(primary: DownloadJob | null, queue: DownloadJob[]): string[] {
  return [primary, ...queue].filter((job): job is DownloadJob => job !== null && isReorderable(job)).map((job) => job.id);
}

export function statusTone(status: string): StatusTone {
  if (status === "failed") return "danger";
  if (status === "installed") return "success";
  if (status === "cancelled") return "neutral";
  if (PROCESSING.includes(status) || status === "queued" || status === "waiting") return "info";
  if (status === "paused") return "warning";
  return "neutral";
}

export function statusBadge(status: string, selected?: Language): string {
  const labels: Record<string, string> = {
    queued: "In coda",
    downloading: "Scaricamento",
    waiting: "In attesa",
    paused: "In pausa",
    failed: "Errore",
    downloaded: "Verifica",
    staging: "Estrazione",
    staged: "Pronto",
    finalizing: "Installazione",
    installed: "Completato",
    cancelled: "Annullato",
  };
  return t(labels[status] ?? status, selected);
}

export function phaseLabel(job: DownloadJob, selected?: Language): string | null {
  if (job.status === "downloading") return `Download ${Math.round(progressOf(job))}%`;
  // Lo stato "in coda" è già descritto dal badge.
  if (job.status === "queued") return null;
  const labels: Record<string, string> = {
    waiting: "In attesa di rete",
    downloaded: "Verifica del file",
    staging: "Estrazione dei file",
    staged: "Pronto per l'installazione",
    finalizing: "Installazione in corso",
    installed: "Installazione completata",
    paused: "Download in pausa",
    failed: "Download interrotto",
    cancelled: "Download annullato",
  };
  return t(labels[job.status] ?? job.status, selected);
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
