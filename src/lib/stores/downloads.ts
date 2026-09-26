import { derived } from "svelte/store";
import {
  cancelDownload,
  isActiveDownloadStatus,
  listDownloads,
  pauseDownload,
  queueDownload,
  resumeDownload,
  retryDownload,
  type DownloadJob,
} from "../services/downloads";
import { createResource } from "./resource";

export const downloads = createResource<DownloadJob[]>([], listDownloads, (jobs) => jobs.length === 0);

export const activeDownloadCount = derived(
  downloads,
  (state) => state.data.filter((job) => isActiveDownloadStatus(job.status)).length,
);

export async function queueJob(steamAppId: number, acceptUnverified: boolean): Promise<void> {
  await queueDownload(steamAppId, acceptUnverified);
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
