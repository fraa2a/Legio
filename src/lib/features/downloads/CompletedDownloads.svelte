<script lang="ts">
  import { t, language } from "../../i18n";
  import type { DownloadJob } from "../../services/downloads";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import { formatBytes } from "../../utils/format";
  import DownloadCover from "./DownloadCover.svelte";
  import { progressOf, statusBadge, statusTone } from "./downloads-model";

  let {
    jobs,
    busyJob = null,
    canPlay,
    onPlay,
    onRemove,
  }: {
    jobs: DownloadJob[];
    busyJob?: string | null;
    canPlay: (job: DownloadJob) => boolean;
    onPlay: (job: DownloadJob) => void;
    onRemove: (job: DownloadJob) => void;
  } = $props();

  function completedOn(job: DownloadJob): string {
    return new Date(job.updatedAt * 1000).toLocaleDateString(undefined, {
      day: "numeric",
      month: "short",
      year: "numeric",
    });
  }
</script>

<ul class="flex flex-col gap-2" role="list">
  {#each jobs as job (job.id)}
    <li class="flex flex-wrap items-center gap-3 rounded-xl bg-white/5 p-3 light:bg-zinc-100">
      <DownloadCover steamAppId={job.steamAppId} name={job.name} class="aspect-[2/3] w-9 rounded-lg" />

      <div class="min-w-0 flex-1 basis-44">
        <div class="flex flex-wrap items-center gap-2">
          <p class="min-w-0 truncate font-medium text-zinc-50 light:text-zinc-900">{job.name}</p>
          <Badge tone={statusTone(job.status)} title={statusBadge(job.status, $language)} />
        </div>
        <p class="mt-0.5 truncate text-xs text-zinc-400 light:text-zinc-600">{t("\n          Versione ", $language)}{job.releaseVersion} · {formatBytes(job.sizeBytes)} · {completedOn(job)}
        </p>
        {#if job.status === "installed"}
          <ProgressBar
            value={progressOf(job)}
            tone="success"
            label="{t("Avanzamento di ", $language)}{job.name}"
            class="mt-2 h-1.5"
          />
        {/if}
        {#if job.error !== null}
          <p class="mt-2 break-words text-xs text-red-300 light:text-red-700" role="alert">{job.error}</p>
        {/if}
      </div>

      <div class="flex shrink-0 flex-wrap items-center justify-end gap-2">
        {#if job.status === "installed"}
          <Button
            label={t("Gioca", $language)}
            variant="play"
            class="h-8! px-3! text-xs!"
            disabled={!canPlay(job)}
            onClick={() => onPlay(job)}
          />
        {/if}
        <Button
          label={t("Rimuovi", $language)}
          variant="secondary"
          class="h-8! px-3! text-xs!"
          disabled={busyJob === job.id}
          onClick={() => onRemove(job)}
        />
      </div>
    </li>
  {/each}
</ul>
