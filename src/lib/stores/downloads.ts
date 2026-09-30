import { derived, writable } from "svelte/store";
import {
  cancelDownload,
  getDownloadBandwidthLimit,
  isActiveDownloadStatus,
  isFinishedDownloadStatus,
  listDownloads,
  openInstalledFolder,
  pauseDownload,
  queueDownload,
  removeDownload,
  removeFinishedDownloads,
  resumeDownload,
  retryDownload,
  setDownloadBandwidthLimit,
  stageDownload,
  type DownloadJob,
} from "../services/downloads";
import { toMessage } from "../utils/errors";
import { createResource } from "./resource";

export const downloads = createResource<DownloadJob[]>([], listDownloads, (jobs) => jobs.length === 0);

export const activeDownloadCount = derived(
  downloads,
  (state) => state.data.filter((job) => isActiveDownloadStatus(job.status)).length,
);

export const pendingDownloadCount = derived(
  downloads,
  (state) => state.data.filter((job) => !isFinishedDownloadStatus(job.status)).length,
);

export const finishedDownloadCount = derived(
  downloads,
  (state) => state.data.filter((job) => isFinishedDownloadStatus(job.status)).length,
);

const statusRank: Record<string, number> = {
  downloading: 0,
  staging: 0,
  finalizing: 0,
  queued: 1,
  waiting: 1,
  paused: 2,
  failed: 3,
  downloaded: 4,
  staged: 4,
  installed: 5,
  cancelled: 6,
};

export const orderedDownloads = derived(downloads, (state) =>
  [...state.data].sort(
    (left, right) =>
      (statusRank[left.status] ?? 4) - (statusRank[right.status] ?? 4),
  ),
);

export const currentDownload = derived(orderedDownloads, (state) => {
  const active = state.find((job) => isActiveDownloadStatus(job.status));
  return active ?? state.find((job) => job.status === "paused") ?? null;
});

export const bandwidthLimit = createResource(0, getDownloadBandwidthLimit);

export const bandwidthLimitError = writable<string | null>(null);

export const installedFolderError = writable<string | null>(null);

let progressTimer: ReturnType<typeof setInterval> | undefined;
let progressPollingSubscribed = false;

function syncProgressPolling(count: number): void {
  if (count === 0) {
    clearInterval(progressTimer);
    progressTimer = undefined;
    return;
  }
  progressTimer ??= setInterval(() => void downloads.load(), 1000);
}

export function startDownloadProgressPolling(): void {
  if (progressPollingSubscribed) return;
  progressPollingSubscribed = true;
  activeDownloadCount.subscribe(syncProgressPolling);
}

export async function queueJob(steamAppId: number, sha256: string, acceptUnverified: boolean): Promise<void> {
  await queueDownload(steamAppId, sha256, acceptUnverified);
  await downloads.load();
}

export async function pauseJob(id: string): Promise<void> {
  await pauseDownload(id);
  await downloads.load();
}

export async function resumeJob(id: string): Promise<void> {
  await resumeDownload(id);
  await downloads.load();
}

export async function retryJob(id: string): Promise<void> {
  await retryDownload(id);
  await downloads.load();
}

export async function cancelJob(id: string): Promise<void> {
  await cancelDownload(id);
  await downloads.load();
}

export async function removeJob(id: string): Promise<void> {
  await removeDownload(id);
  await downloads.load();
}

export async function removeFinishedJobs(): Promise<number> {
  const removed = await removeFinishedDownloads();
  await downloads.load();
  return removed.length;
}

export async function stageJob(id: string): Promise<void> {
  await stageDownload(id);
  await downloads.load();
}

export async function browseInstalledFolder(): Promise<void> {
  installedFolderError.set(null);
  try {
    await openInstalledFolder();
  } catch (error) {
    installedFolderError.set(toMessage(error));
  }
}

export async function saveBandwidthLimit(bytesPerSecond: number): Promise<void> {
  bandwidthLimitError.set(null);
  if (!Number.isSafeInteger(bytesPerSecond) || bytesPerSecond < 0) {
    bandwidthLimitError.set("Il limite di banda deve essere un numero intero di byte al secondo.");
    return;
  }
  try {
    await setDownloadBandwidthLimit(bytesPerSecond);
    bandwidthLimit.set(bytesPerSecond);
  } catch (error) {
    bandwidthLimitError.set(toMessage(error));
  }
}
