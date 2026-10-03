<script lang="ts">
  import {
    canCancelDownload,
    canFinalizeDownload,
    canPauseDownload,
    canRemoveDownload,
    canResumeDownload,
    canRetryDownload,
    type DownloadJob,
  } from "../../services/downloads";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import { formatBytes } from "../../utils/format";
  import DownloadCover from "./DownloadCover.svelte";
  import { formatEta, phaseLabel, progressOf, statusBadge, statusTone } from "./downloads-model";

  let {
    job,
    busy = false,
    onPause,
    onResume,
    onRetry,
    onCancel,
    onRemove,
    onInstall,
  }: {
    job: DownloadJob;
    busy?: boolean;
    onPause: () => void;
    onResume: () => void;
    onRetry: () => void;
    onCancel: () => void;
    onRemove: () => void;
    onInstall: () => void;
  } = $props();

  const progress = $derived(progressOf(job));
  const downloading = $derived(job.status === "downloading");
  const phase = $derived(phaseLabel(job));
  const busyPhase = $derived(
    ["downloading", "downloaded", "staging", "finalizing"].includes(job.status),
  );

  const barTone = $derived.by(() => {
    if (job.status === "failed") return "danger" as const;
    if (job.status === "paused") return "warning" as const;
    if (job.status === "installed") return "success" as const;
    if (downloading) return "accent" as const;
    if (["downloaded", "staging", "staged", "finalizing"].includes(job.status)) return "accent" as const;
    return "default" as const;
  });
</script>

<section
  class="rounded-2xl bg-white/5 p-5 light:bg-zinc-100"
  aria-label="Download attivo di {job.name}"
>
  <div class="flex flex-col gap-5 sm:flex-row sm:items-start">
    <DownloadCover
      steamAppId={job.steamAppId}
      name={job.name}
      class="aspect-[2/3] w-28 self-center sm:w-24 md:w-28"
    />

    <div class="min-w-0 flex-1">
      <div class="flex flex-wrap items-start justify-between gap-x-6 gap-y-2">
        <div class="min-w-0">
          <div class="flex flex-wrap items-center gap-2.5">
            <h2 class="min-w-0 truncate text-xl font-semibold text-zinc-50 light:text-zinc-900">
              {job.name}
            </h2>
            <Badge tone={statusTone(job.status)} title={statusBadge(job.status)} />
          </div>
          <p class="mt-1 flex items-center gap-2 text-sm text-zinc-400 light:text-zinc-600">
            {#if busyPhase}
              <span class="size-1.5 shrink-0 rounded-full bg-legio-download animate-pulse" aria-hidden="true"></span>
            {/if}
            <span class="truncate">{#if phase !== null}{phase} · {/if}Versione {job.releaseVersion}</span>
          </p>
        </div>
        <p class="text-3xl font-semibold tabular-nums text-zinc-50 light:text-zinc-900">
          {Math.round(progress)}%
        </p>
      </div>

      <div class="mt-4">
        <ProgressBar
          value={progress}
          tone={barTone}
          label="Avanzamento di {job.name}"
          class="h-2.5"
        />
        <div class="mt-2.5 flex flex-wrap gap-x-5 gap-y-1 text-xs text-zinc-400 light:text-zinc-600">
          <span class="tabular-nums">{formatBytes(job.downloadedBytes)} / {formatBytes(job.sizeBytes)}</span>
          {#if downloading}
            <span class="tabular-nums">{formatBytes(job.speedBps)}/s</span>
            <span class="tabular-nums">Rimanenti {formatEta(job.etaSeconds)}</span>
          {:else if job.status === "installed"}
            <span>Completato il {new Date(job.updatedAt * 1000).toLocaleDateString("it-IT")}</span>
          {/if}
        </div>
      </div>

      {#if job.error !== null}
        <div class="mt-4 rounded-lg border border-red-500/40 bg-red-500/10 px-3.5 py-2.5" role="alert">
          <p class="text-sm text-red-200 light:text-red-800">{job.error}</p>
        </div>
      {/if}

      <div class="mt-4 flex flex-wrap gap-2">
        {#if canFinalizeDownload(job.status)}
          <Button label="Installa" variant="primary" disabled={busy} onClick={onInstall} />
        {/if}
        {#if canPauseDownload(job.status)}
          <Button label="Pausa" variant="secondary" disabled={busy} onClick={onPause} />
        {/if}
        {#if canResumeDownload(job.status)}
          <Button label="Riprendi" variant="primary" disabled={busy} onClick={onResume} />
        {/if}
        {#if canRetryDownload(job.status)}
          <Button label="Riprova" variant="primary" disabled={busy} onClick={onRetry} />
        {/if}
        {#if canCancelDownload(job.status)}
          <Button label="Annulla" variant="danger" disabled={busy} onClick={onCancel} />
        {/if}
        {#if canRemoveDownload(job.status)}
          <Button label="Rimuovi" variant="secondary" disabled={busy} onClick={onRemove} />
        {/if}
      </div>
    </div>
  </div>
</section>
