<script lang="ts">
  import { t, language } from "../../i18n";
  import {
    canCancelDownload,
    canPauseDownload,
    canResumeDownload,
    downloadProgressPercent,
    type DownloadJob,
  } from "../../services/downloads";
  import { phaseLabel } from "../../features/downloads/downloads-model";
  import { currentDownload, pauseJob, resumeJob } from "../../stores/downloads";
  import { selectSection } from "../../stores/navigation";
  import { toMessage } from "../../utils/errors";
  import { formatBytes } from "../../utils/format";
  import CancelDownloadDialog from "../../features/downloads/CancelDownloadDialog.svelte";
  import Icon, { type IconName } from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import WaveFill from "../ui/WaveFill.svelte";
  import DownloadCover from "../../features/downloads/DownloadCover.svelte";

  type QuickAction = {
    icon: IconName;
    label: string;
    title: string;
    trigger: () => void;
  };

  let { expanded }: { expanded: boolean } = $props();

  let actionError = $state<string | null>(null);
  let busy = $state(false);
  let cancelTarget = $state<DownloadJob | null>(null);

  const lead = $derived($currentDownload);
  const percent = $derived(lead === null ? 0 : downloadProgressPercent(lead));
  const percentLabel = $derived(`${Math.round(percent)}%`);
  const moving = $derived(lead !== null && lead.status !== "paused" && lead.status !== "waiting" && lead.status !== "failed");

  const detailLabel = $derived.by(() => {
    if (lead === null) return "";
    if (lead.pauseReason === null && lead.status === "downloading") {
      return `${formatBytes(lead.downloadedBytes, $language)} / ${formatBytes(lead.sizeBytes, $language)}`;
    }
    return phaseLabel(lead, $language) ?? `${formatBytes(lead.downloadedBytes, $language)} / ${formatBytes(lead.sizeBytes, $language)}`;
  });

  const quickActions: QuickAction[] = $derived.by(() => {
    if (lead === null) return [];
    const actions: QuickAction[] = [];
    if (canResumeDownload(lead.status) && lead.pauseReason !== "game_running") {
      actions.push({
        icon: "play",
        label: t("Riprendi il download di {0}", $language, [lead.name]),
        title: t("Riprendi", $language),
        trigger: () => void run(resumeJob),
      });
    }
    if (canPauseDownload(lead.status)) {
      actions.push({
        icon: "pause",
        label: t("Metti in pausa il download di {0}", $language, [lead.name]),
        title: t("Pausa", $language),
        trigger: () => void run(pauseJob),
      });
    }
    if (canCancelDownload(lead.status)) {
      actions.push({
        icon: "stop",
        label: t("Annulla il download di {0}", $language, [lead.name]),
        title: t("Annulla", $language),
        trigger: () => (cancelTarget = lead),
      });
    }
    return actions;
  });

  async function run(action: (id: string) => Promise<void>): Promise<void> {
    if (lead === null) return;
    actionError = null;
    busy = true;
    try {
      await action(lead.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      busy = false;
    }
  }
</script>

{#if lead}
  {#if expanded}
    <div
      role="group"
      aria-label={t("Download in corso: {0}", $language, [lead.name])}
      class="group/download relative overflow-hidden flex shrink-0 flex-col gap-2 rounded-lg border border-white/10 bg-white/5 p-2 light:border-zinc-900/15 light:bg-zinc-100"
    >
      <WaveFill value={percent} active={moving} />
      <div class="relative flex min-h-7 items-center gap-2">
        <DownloadCover steamAppId={lead.steamAppId} name={lead.name} class="h-12 w-8" rounded="md" />
        <p class="min-w-0 flex-1 truncate text-xs font-medium text-white light:text-zinc-900">
          {lead.name}
        </p>
        {#if quickActions.length > 0}
          <div class="pointer-events-none flex shrink-0 items-center gap-1 opacity-0 group-hover/download:pointer-events-auto group-hover/download:opacity-100 group-focus-within/download:pointer-events-auto group-focus-within/download:opacity-100">
            {#each quickActions as action (action.title)}
              <IconButton size="sm" variant="soft" disabled={busy} label={action.label} title={action.title} onClick={action.trigger}>
                <Icon name={action.icon} size="size-3.5" />
              </IconButton>
            {/each}
          </div>
        {/if}
        <span
          role="progressbar" aria-label={t("Avanzamento di {0}", $language, [lead.name])} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(percent)}
          class="shrink-0 text-xs font-semibold text-zinc-300 light:text-zinc-700 {quickActions.length > 0
            ? 'pointer-events-none absolute right-0 group-hover/download:opacity-0 group-focus-within/download:opacity-0'
            : ''}"
        >
          {percentLabel}
        </span>
      </div>


      <div
        class="relative flex items-center justify-between gap-2 text-[0.6875rem] text-zinc-300 light:text-zinc-700"
      >
        <span class="truncate">{detailLabel}</span>
        {#if lead.speedBps > 0}
          <span class="shrink-0">{formatBytes(lead.speedBps, $language)}/s</span>
        {/if}
      </div>

      {#if actionError}
        <p class="relative text-[0.6875rem] text-legio-error" role="alert">{actionError}</p>
      {/if}
    </div>
  {:else}
    <button
      type="button"
      aria-label={t("Download in corso: {0}, {1}. Apri la sezione Download", $language, [lead.name, percentLabel])}
      onclick={() => selectSection("downloads")}
      class="relative flex size-12 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-white/5 text-zinc-200 transition-colors duration-200 hover:bg-white/10 hover:text-white light:bg-zinc-900/5 light:text-zinc-500 light:hover:bg-zinc-900/10 light:hover:text-zinc-900"
    >
      <WaveFill compact value={percent} active={moving} />
      <span class="relative">
        <Icon name={lead.pauseReason === "game_running" ? "pause" : "downloads"} size="h-5 w-5" />
      </span>
    </button>
  {/if}
{/if}

{#if cancelTarget}
  <CancelDownloadDialog
    open
    jobId={cancelTarget.id}
    gameName={cancelTarget.name}
    onClose={() => (cancelTarget = null)}
  />
{/if}
