<script lang="ts">
  import {
    canCancelDownload,
    canFinalizeDownload,
    canPauseDownload,
    canRemoveDownload,
    canResumeDownload,
    canRetryDownload,
    canStageDownload,
    describeDownloadStatus,
    isActiveDownloadStatus,
    type DownloadJob,
  } from "../../services/downloads";
  import { toMessage } from "../../utils/errors";
  import { formatBytes } from "../../utils/format";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import {
    cancelJob,
    downloads,
    finishedDownloadCount,
    orderedDownloads,
    pauseJob,
    removeFinishedJobs,
    removeJob,
    resumeJob,
    retryJob,
    stageJob,
  } from "../../stores/downloads";
  import { openStagedInstall, stagedInstall } from "../../stores/staged-install";
  import StagedInstallDialog from "./StagedInstallDialog.svelte";

  let actionError = $state<string | null>(null);
  let pendingJob = $state<string | null>(null);
  let stagingJob = $state<string | null>(null);
  let cancelTarget = $state<DownloadJob | null>(null);
  let removeTarget = $state<DownloadJob | null>(null);
  let clearingFinished = $state(false);

  const hasActiveJobs = $derived($downloads.data.some((job) => isActiveDownloadStatus(job.status)));

  const installTarget = $derived(
    $downloads.data.find((job) => job.id === $stagedInstall.jobId) ?? null,
  );

  $effect(() => {
    if (!hasActiveJobs) return;
    const timer = setInterval(() => void downloads.load(), 2000);
    return () => clearInterval(timer);
  });

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

  const progressPercent = (job: DownloadJob): number =>
    job.sizeBytes > 0 ? Math.min(100, (job.downloadedBytes / job.sizeBytes) * 100) : 0;

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

  async function confirmCancel(): Promise<void> {
    const job = cancelTarget;
    cancelTarget = null;
    if (job === null) return;
    await run(cancelJob, job);
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

  async function stage(job: DownloadJob): Promise<void> {
    actionError = null;
    stagingJob = job.id;
    try {
      await stageJob(job.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      stagingJob = null;
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

        <div
          class="h-1.5 w-full overflow-hidden rounded-full bg-white/10 light:bg-zinc-900/10"
          role="progressbar"
          aria-label="Avanzamento di {job.name}"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={Math.round(progressPercent(job))}
        >
          <div
            class="h-full rounded-full bg-zinc-100 transition-[width] duration-300 light:bg-zinc-900"
            style="width: {progressPercent(job)}%"
          ></div>
        </div>

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

        {#if stagingJob === job.id}
          <p role="status">
            <span
              class="size-4 shrink-0 animate-spin rounded-full border-2 border-zinc-400 border-t-transparent"
              aria-hidden="true"
            ></span>
          </p>
        {/if}

        <div class="flex flex-wrap gap-2">
          {#if canStageDownload(job.status)}
            <Button
              label="Estrai e verifica"
              variant="primary"
              disabled={pendingJob === job.id || stagingJob === job.id}
              onClick={() => void stage(job)}
            />
          {/if}
          {#if canFinalizeDownload(job.status)}
            <Button
              label="Installa"
              disabled={pendingJob === job.id || stagingJob === job.id}
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

<Dialog open={cancelTarget !== null} title="Annulla download" onClose={() => (cancelTarget = null)}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Annullare il download di {cancelTarget?.name}?
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Indietro" variant="secondary" onClick={() => (cancelTarget = null)} />
    <Button label="Annulla" variant="danger" onClick={() => void confirmCancel()} />
  </div>
</Dialog>

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
