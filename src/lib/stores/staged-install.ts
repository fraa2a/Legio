import { get, writable } from "svelte/store";
import {
  finalizeDownload,
  scanStagedExecutables,
  type StagedExecutableCandidate,
} from "../services/downloads";
import { toMessage } from "../utils/errors";
import { downloads } from "./downloads";
import { games } from "./games";
import type { LoadStatus } from "./resource";

interface StagedInstallState {
  jobId: string | null;
  status: LoadStatus;
  candidates: StagedExecutableCandidate[];
  selectedPath: string | null;
  finalizing: boolean;
  error: string | null;
}

const initial: StagedInstallState = {
  jobId: null,
  status: "idle",
  candidates: [],
  selectedPath: null,
  finalizing: false,
  error: null,
};

export const stagedInstall = writable<StagedInstallState>(initial);

let scanRequest = 0;

export function resetStagedInstall(): void {
  scanRequest += 1;
  stagedInstall.set(initial);
}

export function chooseStagedCandidate(relativePath: string): void {
  stagedInstall.update((state) => ({ ...state, selectedPath: relativePath }));
}

async function scan(jobId: string): Promise<void> {
  const request = ++scanRequest;
  stagedInstall.update((state) => ({ ...state, status: "loading", error: null }));
  try {
    const scan = await scanStagedExecutables(jobId, null);
    if (request !== scanRequest) return;
    stagedInstall.update((state) => ({
      ...state,
      status: scan.candidates.length === 0 ? "empty" : "ready",
      candidates: scan.candidates,
      selectedPath: scan.selectedRelativePath,
    }));
  } catch (error) {
    if (request !== scanRequest) return;
    stagedInstall.update((state) => ({
      ...state,
      status: "error",
      candidates: [],
      selectedPath: null,
      error: toMessage(error),
    }));
  }
}

export async function openStagedInstall(jobId: string): Promise<void> {
  scanRequest += 1;
  stagedInstall.set({ ...initial, jobId });
  await scan(jobId);
}

export async function rescanStagedCandidates(): Promise<void> {
  const jobId = get(stagedInstall).jobId;
  if (jobId === null) return;
  await scan(jobId);
}

export async function finalizeStagedInstall(): Promise<boolean> {
  const { jobId, selectedPath } = get(stagedInstall);
  if (jobId === null || selectedPath === null) return false;

  stagedInstall.update((state) => ({ ...state, finalizing: true, error: null }));
  let failure: string | null = null;
  try {
    await finalizeDownload(jobId, selectedPath);
  } catch (error) {
    failure = toMessage(error);
  }
  stagedInstall.update((state) => ({ ...state, finalizing: false, error: failure }));
  await downloads.load();
  if (failure !== null) return false;

  await games.load();
  resetStagedInstall();
  return true;
}
