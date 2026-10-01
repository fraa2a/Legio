<script lang="ts">
  import {
    canCancelDownload,
    canFinalizeDownload,
    canPauseDownload,
    canRemoveDownload,
    canResumeDownload,
    canRetryDownload,
    describeDownloadStatus,
    downloadProgressPercent,
    isActiveDownloadStatus,
    type DownloadJob,
  } from "../../services/downloads";
  import { toMessage } from "../../utils/errors";
  import { formatBytes } from "../../utils/format";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import {
    downloads,
    finishedDownloadCount,
    orderedDownloads,
    pauseJob,
    removeFinishedJobs,
    removeJob,
    resumeJob,
    retryJob,
  } from "../../stores/downloads";
  import { openStagedInstall, stagedInstall } from "../../stores/staged-install";
  import CancelDownloadDialog from "./CancelDownloadDialog.svelte";
  import StagedInstallDialog from "./StagedInstallDialog.svelte";

  let actionError = $state<string | null>(null);
  let pendingJob = $state<string | null>(null);
  let cancelTarget = $state<DownloadJob | null>(null);
  let removeTarget = $state<DownloadJob | null>(null);
  let clearingFinished = $state(false);

  const installTarget = $derived(
    $downloads.data.find((job) => job.id === $stagedInstall.jobId) ?? null,
  );

  const formatEta = (seconds: number | null): string => {
    if (seconds === null) return "-";
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    return `${minutes}m ${seconds % 60}s`;
  };

  const statusTone = (job: DownloadJob): "neutral" | "info" | "success" | "danger" | "warning" => {
    if (job.status === "failed") return "danger";
    if (job.status === "installed") return "success";
    if (job.status === "cancelled") return "neutral";
    if (isActiveDownloadStatus(job.status)) return "info";
    if (job.status === "downloaded" || job.status === "staged") return "warning";
    return "neutral";
  };

  async function run(action: (id: string) => Promise<void>, job: DownloadJob): Promise<void> {
    actionError = null;
    pendingJob = job.id;
    try {
      await action(job.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pendingJob = null;
    }
  }

  async function confirmRemove(): Promise<void> {
    const job = removeTarget;
    removeTarget = null;
    if (job === null) return;
    await run(removeJob, job);
  }

  async function clearFinished(): Promise<void> {
    actionError = null;
    clearingFinished = true;
    try {
      await removeFinishedJobs();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      clearingFinished = false;
    }
  }
</script>

{#if actionError}
  <div class="mb-4">
    <ErrorBanner message={actionError} />
  </div>
{/if}

<StateBlock
  status={$downloads.status}
  hasData={$downloads.data.length > 0}
  emptyMessage="Nessun download in coda."
  error={$downloads.error}
  onRetry={() => void downloads.load()}
/>

{#if $downloads.data.length > 0}
  {#if $finishedDownloadCount > 0}
    <div class="mb-4 flex justify-end">
      <Button
        label="Rimuovi completati ({$finishedDownloadCount})"
        variant="secondary"
        disabled={clearingFinished || pendingJob !== null}
        onClick={() => void clearFinished()}
      />
    </div>
  {/if}

  <ul class="flex flex-col gap-3">
    {#each $orderedDownloads as job (job.id)}
      <li class="flex flex-col gap-3 rounded-xl bg-white/5 p-4 light:bg-zinc-100">
        <div class="flex flex-wrap items-center gap-3">
          <p class="min-w-0 flex-1 truncate font-medium text-zinc-50 light:text-zinc-900">{job.name}</p>
          <Badge tone={statusTone(job)} title={describeDownloadStatus(job.status)} />
        </div>

        <ProgressBar value={downloadProgressPercent(job)} label="Avanzamento di {job.name}" />

        <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-zinc-400 light:text-zinc-600">
          <span>Versione {job.releaseVersion}</span>
          <span>{formatBytes(job.downloadedBytes)} / {formatBytes(job.sizeBytes)}</span>
          {#if job.status === "downloading" || job.status === "queued" || job.status === "waiting"}
            <span>{formatBytes(job.speedBps)}/s</span>
            <span>Stima {formatEta(job.etaSeconds)}</span>
          {/if}
        </div>

        {#if job.error}
          <p class="text-xs text-red-300 light:text-red-700" role="alert">{job.error}</p>
        {/if}

        <div class="flex flex-wrap gap-2">
          {#if canFinalizeDownload(job.status)}
            <Button
              label="Installa"
              variant="primary"
              disabled={pendingJob === job.id}
              onClick={() => void openStagedInstall(job.id)}
            />
          {/if}
          {#if canPauseDownload(job.status)}
            <Button
              label="Pausa"
              variant="secondary"
              disabled={pendingJob === job.id}
              onClick={() => void run(pauseJob, job)}
            />
          {/if}
          {#if canResumeDownload(job.status)}
            <Button
              label="Riprendi"
              variant="secondary"
              disabled={pendingJob === job.id}
              onClick={() => void run(resumeJob, job)}
            />
          {/if}
          {#if canRetryDownload(job.status)}
            <Button
              label="Riprova"
              variant="secondary"
              disabled={pendingJob === job.id}
              onClick={() => void run(retryJob, job)}
            />
          {/if}
          {#if canCancelDownload(job.status)}
            <Button
              label="Annulla"
              variant="danger"
              disabled={pendingJob === job.id}
              onClick={() => (cancelTarget = job)}
            />
          {/if}
          {#if canRemoveDownload(job.status)}
            <Button
              label="Rimuovi dalla coda"
              variant="secondary"
              disabled={pendingJob === job.id}
              onClick={() => (removeTarget = job)}
            />
          {/if}
        </div>
      </li>
    {/each}
  </ul>
{/if}

{#if cancelTarget}
  <CancelDownloadDialog
    open
    jobId={cancelTarget.id}
    gameName={cancelTarget.name}
    onClose={() => (cancelTarget = null)}
  />
{/if}

<Dialog open={removeTarget !== null} title="Rimuovi dalla coda" onClose={() => (removeTarget = null)}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Rimuovere {removeTarget?.name} dalla coda?
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Indietro" variant="secondary" onClick={() => (removeTarget = null)} />
    <Button label="Rimuovi" variant="danger" onClick={() => void confirmRemove()} />
  </div>
</Dialog>

{#if installTarget !== null}
  <StagedInstallDialog gameName={installTarget.name} status={installTarget.status} />
{/if}
