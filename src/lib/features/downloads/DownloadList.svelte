<script lang="ts">
  import { t, language } from "../../i18n";
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
  import Icon from "../../components/ui/Icon.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import { formatBytes } from "../../utils/format";
  import DownloadCover from "./DownloadCover.svelte";
  import { isReorderable, phaseLabel, progressOf, statusBadge, statusTone } from "./downloads-model";

  let {
    jobs,
    reorder = false,
    positions = false,
    positionOffset = 0,
    busyJob = null,
    onReorder,
    onPause,
    onResume,
    onRetry,
    onCancel,
    onRemove,
    onInstall,
  }: {
    jobs: DownloadJob[];
    reorder?: boolean;
    positions?: boolean;
    positionOffset?: number;
    busyJob?: string | null;
    onReorder?: (ids: string[]) => void;
    onPause: (job: DownloadJob) => void;
    onResume: (job: DownloadJob) => void;
    onRetry: (job: DownloadJob) => void;
    onCancel: (job: DownloadJob) => void;
    onRemove: (job: DownloadJob) => void;
    onInstall: (job: DownloadJob) => void;
  } = $props();

  let fromIndex = $state<number | null>(null);
  let overIndex = $state<number | null>(null);

  function reset(): void {
    fromIndex = null;
    overIndex = null;
  }

  function move(from: number, to: number): void {
    if (onReorder === undefined || from === to || to < 0 || to > jobs.length) return;
    const next = [...jobs];
    const [moved] = next.splice(from, 1);
    next.splice(to, 0, moved);
    onReorder(next.map((job) => job.id));
  }

  function start(index: number, event: DragEvent): void {
    if (!reorder) return;
    fromIndex = index;
    event.dataTransfer?.setData("text/plain", String(index));
    if (event.dataTransfer !== null) event.dataTransfer.effectAllowed = "move";
  }

  function over(index: number, event: DragEvent): void {
    if (fromIndex === null) return;
    event.preventDefault();
    overIndex = index;
  }

  function drop(index: number, event: DragEvent): void {
    event.preventDefault();
    const from = fromIndex;
    reset();
    if (from === null) return;
    move(from, index);
  }

  function dropAtEnd(event: DragEvent): void {
    if (event.target !== event.currentTarget) return;
    drop(jobs.length, event);
  }

  function barTone(job: DownloadJob): "default" | "accent" | "warning" | "danger" | "success" {
    if (job.status === "failed") return "danger";
    if (job.status === "paused") return "warning";
    if (job.status === "installed") return "success";
    if (job.status === "downloading") return "accent";
    if (["downloaded", "staging", "staged", "finalizing"].includes(job.status)) return "accent";
    return "default";
  }
</script>

<ul
  class="flex flex-col gap-2"
  role="list"
  ondragover={(event) => {
    if (event.target === event.currentTarget) over(jobs.length, event);
  }}
  ondrop={dropAtEnd}
>
  {#each jobs as job, index (job.id)}
    {@const phase = phaseLabel(job)}
    <li
      class="group flex flex-wrap items-center gap-3 rounded-xl bg-white/5 p-3 transition-colors duration-200 light:bg-zinc-100
        {fromIndex === index ? 'opacity-40' : ''}
        {overIndex === index && fromIndex !== index ? 'ring-2 ring-legio-download' : ''}
        {reorder && isReorderable(job) ? 'cursor-grab active:cursor-grabbing' : ''}"
      draggable={reorder && isReorderable(job)}
      ondragstart={(event) => start(index, event)}
      ondragover={(event) => over(index, event)}
      ondrop={(event) => drop(index, event)}
      ondragend={reset}
      ondragleave={() => {
        if (overIndex === index) overIndex = null;
      }}
    >
      {#if positions}
        <span class="w-5 shrink-0 text-center text-xs font-medium tabular-nums text-zinc-500 light:text-zinc-400">
          {positionOffset + index + 1}
        </span>
      {/if}

      <DownloadCover steamAppId={job.steamAppId} name={job.name} class="aspect-[2/3] w-9 rounded-lg" />

      <div class="min-w-0 flex-1 basis-44">
        <div class="flex flex-wrap items-center gap-2">
          <p class="min-w-0 truncate font-medium text-zinc-50 light:text-zinc-900">{job.name}</p>
          <Badge tone={statusTone(job.status)} title={statusBadge(job.status)} />
        </div>
        <p class="mt-0.5 truncate text-xs text-zinc-400 light:text-zinc-600">{t("\n          Versione ", $language)}{job.releaseVersion} · {formatBytes(job.sizeBytes)}{#if phase !== null} · {phase}{/if}
        </p>
        {#if job.downloadedBytes > 0}
          <ProgressBar
            value={progressOf(job)}
            tone={barTone(job)}
            label="{t("Avanzamento di ", $language)}{job.name}"
            class="mt-2 h-1.5"
          />
        {/if}
        {#if job.error !== null}
          <p class="mt-2 break-words text-xs text-red-300 light:text-red-700" role="alert">{job.error}</p>
        {/if}
      </div>

      <div class="flex shrink-0 flex-wrap items-center justify-end gap-2">
        {#if canFinalizeDownload(job.status)}
          <Button
            label={t("Installa", $language)}
            variant="primary"
            class="h-8! px-3! text-xs!"
            disabled={busyJob === job.id}
            onClick={() => onInstall(job)}
          />
        {/if}
        {#if canPauseDownload(job.status)}
          <Button
            label={t("Pausa", $language)}
            variant="secondary"
            class="h-8! px-3! text-xs!"
            disabled={busyJob === job.id}
            onClick={() => onPause(job)}
          />
        {/if}
        {#if canResumeDownload(job.status)}
          <Button
            label={t("Riprendi", $language)}
            variant="secondary"
            class="h-8! px-3! text-xs!"
            disabled={busyJob === job.id}
            onClick={() => onResume(job)}
          />
        {/if}
        {#if canRetryDownload(job.status)}
          <Button
            label={t("Riprova", $language)}
            variant="primary"
            class="h-8! px-3! text-xs!"
            disabled={busyJob === job.id}
            onClick={() => onRetry(job)}
          />
        {/if}
        {#if canCancelDownload(job.status)}
          <Button
            label={t("Annulla", $language)}
            variant="danger"
            class="h-8! px-3! text-xs!"
            disabled={busyJob === job.id}
            onClick={() => onCancel(job)}
          />
        {/if}
        {#if canRemoveDownload(job.status)}
          <Button
            label={t("Rimuovi", $language)}
            variant="secondary"
            class="h-8! px-3! text-xs!"
            disabled={busyJob === job.id}
            onClick={() => onRemove(job)}
          />
        {/if}
      </div>

      {#if reorder && onReorder !== undefined && isReorderable(job)}
        <div
          class="ml-auto flex shrink-0 flex-col gap-0.5 opacity-0 transition-opacity duration-200 group-hover:opacity-100 group-focus-within:opacity-100"
        >
          <button
            type="button"
            class="flex size-5 items-center justify-center rounded text-zinc-500 transition-colors duration-200 hover:bg-white/10 hover:text-zinc-200 disabled:opacity-30 disabled:hover:bg-transparent light:text-zinc-400 light:hover:bg-zinc-200 light:hover:text-zinc-800"
            aria-label="{t("Sposta ", $language)}{job.name}{t(" in alto", $language)}"
            title={t("Sposta in alto", $language)}
            disabled={index === 0 || busyJob !== null}
            onclick={() => move(index, index - 1)}
          >
            <Icon name="chevron-down" size="size-3.5 -rotate-90" />
          </button>
          <button
            type="button"
            class="flex size-5 items-center justify-center rounded text-zinc-500 transition-colors duration-200 hover:bg-white/10 hover:text-zinc-200 disabled:opacity-30 disabled:hover:bg-transparent light:text-zinc-400 light:hover:bg-zinc-200 light:hover:text-zinc-800"
            aria-label="{t("Sposta ", $language)}{job.name}{t(" in basso", $language)}"
            title={t("Sposta in basso", $language)}
            disabled={index === jobs.length - 1 || busyJob !== null}
            onclick={() => move(index, index + 1)}
          >
            <Icon name="chevron-down" size="size-3.5" />
          </button>
        </div>
      {/if}
    </li>
  {/each}
</ul>
