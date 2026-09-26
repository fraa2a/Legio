<script lang="ts">
  import {
    canCancelDownload,
    canPauseDownload,
    canResumeDownload,
    canRetryDownload,
    describeDownloadStatus,
    isActiveDownloadStatus,
    type DownloadJob,
  } from "../../services/downloads";
  import { toMessage } from "../../utils/errors";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import {
    cancelJob,
    downloads,
    pauseJob,
    resumeJob,
    retryJob,
  } from "../../stores/downloads";

  let actionError = $state<string | null>(null);
  let pendingJob = $state<string | null>(null);
  let cancelTarget = $state<DownloadJob | null>(null);

  const hasActiveJobs = $derived($downloads.data.some((job) => isActiveDownloadStatus(job.status)));

  $effect(() => {
    if (!hasActiveJobs) return;
    const timer = setInterval(() => void downloads.load(), 2000);
    return () => clearInterval(timer);
  });

  const formatBytes = (value: number): string => {
    if (value < 1024) return `${value} B`;
    const units = ["KB", "MB", "GB", "TB"];
    let size = value / 1024;
    let unit = 0;
    while (size >= 1024 && unit < units.length - 1) {
      size /= 1024;
      unit += 1;
    }
    return `${size.toFixed(1)} ${units[unit]}`;
  };

  const formatEta = (seconds: number | null): string => {
    if (seconds === null) return "-";
    if (seconds < 60) return `${seconds}s`;
    const minutes = Math.floor(seconds / 60);
    return `${minutes}m ${seconds % 60}s`;
  };

  const statusTone = (job: DownloadJob): "neutral" | "info" | "success" | "danger" => {
    if (job.status === "failed") return "danger";
    if (job.status === "installed") return "success";
    if (isActiveDownloadStatus(job.status)) return "info";
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
  <ul class="flex flex-col gap-3">
    {#each $downloads.data as job (job.id)}
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
          <span>{formatBytes(job.speedBps)}/s</span>
          <span>Stima {formatEta(job.etaSeconds)}</span>
        </div>

        {#if job.error}
          <p class="text-xs text-red-300 light:text-red-700" role="alert">{job.error}</p>
        {/if}

        <div class="flex flex-wrap gap-2">
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
        </div>
      </li>
    {/each}
  </ul>
{/if}

<Dialog open={cancelTarget !== null} title="Annulla download" onClose={() => (cancelTarget = null)}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Il download di {cancelTarget?.name} verrà annullato e i dati parziali rimossi.
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Indietro" variant="secondary" onClick={() => (cancelTarget = null)} />
    <Button label="Annulla" variant="danger" onClick={() => void confirmCancel()} />
  </div>
</Dialog>
