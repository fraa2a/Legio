import { downloadState } from "../services/download-status";
import { gameUpdates } from "./game-updates";
import { games } from "./games";
import { windowActive } from "./window-activity";
import { t } from "../i18n";
import { derived, get, writable } from "svelte/store";
import {
  cancelDownload,
  getDownloadBandwidthLimit,
  getInstalledFolderInfo,
  isActiveDownloadStatus,
  isFinishedDownloadStatus,
  listDownloads,
  openInstalledFolder,
  pauseDownload,
  queueDownload,
  removeDownload,
  removeFinishedDownloads,
  reorderDownloads,
  resumeDownload,
  retryDownload,
  setDownloadBandwidthLimit,
  type DownloadJob,
  type InstalledFolderInfo,
} from "../services/downloads";
import { toMessage } from "../utils/errors";
import { createResource } from "./resource";

export const downloads = createResource<DownloadJob[]>([], listDownloads, (jobs) => jobs.length === 0);

let installedRevision = "";
downloads.subscribe(state => {
  if (state.status !== "ready") return;
  const revision = state.data.filter(job => job.status === "installed").map(job => `${job.id}:${job.updatedAt}`).join("|");
  if (revision === installedRevision) return;
  installedRevision = revision;
  void Promise.all([games.reload(), gameUpdates.reload()]);
});

export const activeDownloadCount = derived(
  downloads,
  (state) => state.data.filter((job) => (isActiveDownloadStatus(job.status) || job.pauseReason != null)).length,
);

export const finishedDownloadCount = derived(
  downloads,
  (state) => state.data.filter((job) => isFinishedDownloadStatus(job.status)).length,
);

export const orderedDownloads = derived(downloads, (state) =>
  [...state.data].sort(
    (left, right) =>
      (downloadState(left.status)?.rank ?? 4) - (downloadState(right.status)?.rank ?? 4),
  ),
);

export const currentDownload = derived(orderedDownloads, (state) => {
  const active = state.find((job) => isActiveDownloadStatus(job.status));
  return active ?? state.find((job) => job.status === "paused") ?? null;
});

export const bandwidthLimit = createResource(0, getDownloadBandwidthLimit);

export const installedFolder = createResource<InstalledFolderInfo | null>(
  null,
  getInstalledFolderInfo,
  (info) => info === null,
);

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
  windowActive.subscribe((active) => {
    if (active) { void downloads.load(); void gameUpdates.load(); }
    syncProgressPolling(active ? get(activeDownloadCount) : 0);
  });
  activeDownloadCount.subscribe((count) => syncProgressPolling(get(windowActive) ? count : 0));
}

export async function queueJob(steamAppId: number, downloadUrl: string, releaseVersion: string, acceptUnverified: boolean, sourceId?: string | null, preferredUri?: string | null, libraryId?: string | null): Promise<void> {
  await queueDownload(steamAppId, downloadUrl, releaseVersion, acceptUnverified, sourceId, preferredUri, libraryId);
  await downloads.reload();
}

export async function pauseJob(id: string): Promise<void> {
  await pauseDownload(id);
  await downloads.reload();
}

export async function resumeJob(id: string): Promise<void> {
  await resumeDownload(id);
  await downloads.reload();
}

export async function retryJob(id: string): Promise<void> {
  await retryDownload(id);
  await Promise.all([downloads.reload(), games.reload(), installedFolder.reload()]);
}

export async function cancelJob(id: string): Promise<void> {
  await cancelDownload(id);
  await downloads.reload();
}

export async function removeJob(id: string): Promise<void> {
  await removeDownload(id);
  await downloads.reload();
}

export async function removeFinishedJobs(): Promise<number> {
  const removed = await removeFinishedDownloads();
  await downloads.reload();
  return removed.length;
}

export async function reorderJobs(ids: string[]): Promise<void> {
  await reorderDownloads(ids);
  await downloads.reload();
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
    bandwidthLimitError.set(t("Il limite di banda deve essere un numero intero di byte al secondo.", undefined));
    return;
  }
  try {
    await setDownloadBandwidthLimit(bytesPerSecond);
    bandwidthLimit.set(bytesPerSecond);
  } catch (error) {
    bandwidthLimitError.set(toMessage(error));
  }
}
