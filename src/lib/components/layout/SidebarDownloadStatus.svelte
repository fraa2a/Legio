<script lang="ts">
  import { t, language } from "../../i18n";
  import {
    canCancelDownload,
    canPauseDownload,
    canResumeDownload,
    downloadProgressPercent,
    type DownloadJob,
  } from "../../services/downloads";
  import { currentDownload, pauseJob, resumeJob } from "../../stores/downloads";
  import { selectSection } from "../../stores/navigation";
  import { toMessage } from "../../utils/errors";
  import { formatBytes } from "../../utils/format";
  import CancelDownloadDialog from "../../features/downloads/CancelDownloadDialog.svelte";
  import Icon, { type IconName } from "../ui/Icon.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";

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

  const phaseLabels: Record<string, string> = $derived({
    downloaded: t("In attesa di estrazione", $language),
    staging: t("Estrazione", $language),
    finalizing: t("Installazione", $language),
    waiting: t("In attesa", $language),
    paused: t("In pausa", $language),
  });

  const quickActionClass =
    "flex size-7 shrink-0 items-center justify-center rounded-md bg-white/10 text-zinc-200 transition-colors duration-150 hover:bg-white/25 hover:text-white focus-visible:bg-white/25 focus-visible:text-white focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white/80 disabled:opacity-40 light:bg-zinc-900/10 light:text-zinc-700 light:hover:bg-zinc-900/20 light:hover:text-zinc-900 light:focus-visible:bg-zinc-900/20 light:focus-visible:outline-zinc-900/60";

  const lead = $derived($currentDownload);
  const percent = $derived(lead === null ? 0 : downloadProgressPercent(lead));
  const percentLabel = $derived(`${Math.round(percent)}%`);

  const detailLabel = $derived.by(() => {
    if (lead === null) return "";
    return (
      phaseLabels[lead.status] ??
      `${formatBytes(lead.downloadedBytes)} / ${formatBytes(lead.sizeBytes)}`
    );
  });

  const quickActions: QuickAction[] = $derived.by(() => {
    if (lead === null) return [];
    const actions: QuickAction[] = [];
    if (canResumeDownload(lead.status)) {
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
      aria-label="{t("Download in corso: ", $language)}{lead.name}"
      class="group/download flex shrink-0 flex-col gap-2 rounded-lg border border-white/10 bg-white/5 p-2 light:border-zinc-900/15 light:bg-zinc-100"
    >
      <div class="relative flex min-h-7 items-center gap-2">
        <p class="min-w-0 flex-1 truncate text-xs font-medium text-white light:text-zinc-900">
          {lead.name}
        </p>
        {#if quickActions.length > 0}
          <div class="pointer-events-none flex shrink-0 items-center gap-1 opacity-0 group-hover/download:pointer-events-auto group-hover/download:opacity-100 group-focus-within/download:pointer-events-auto group-focus-within/download:opacity-100">
            {#each quickActions as action (action.title)}
              <button
                type="button"
                class={quickActionClass}
                disabled={busy}
                aria-label={action.label}
                title={action.title}
                onclick={action.trigger}
              >
                <Icon name={action.icon} size="h-3.5 w-3.5" />
              </button>
            {/each}
          </div>
        {/if}
        <span
          class="shrink-0 text-xs font-semibold text-zinc-300 light:text-zinc-700 {quickActions.length > 0
            ? 'pointer-events-none absolute right-0 group-hover/download:opacity-0 group-focus-within/download:opacity-0'
            : ''}"
        >
          {percentLabel}
        </span>
      </div>

      <ProgressBar value={percent} label="{t("Avanzamento di ", $language)}{lead.name}" />

      <div
        class="flex items-center justify-between gap-2 text-[0.65rem] text-zinc-400 light:text-zinc-600"
      >
        <span class="truncate">{detailLabel}</span>
        {#if lead.speedBps > 0}
          <span class="shrink-0">{formatBytes(lead.speedBps)}/s</span>
        {/if}
      </div>

      {#if actionError}
        <p class="text-[0.65rem] text-red-300 light:text-red-700" role="alert">{actionError}</p>
      {/if}
    </div>
  {:else}
    <button
      type="button"
      aria-label="{t("Download in corso: ", $language)}{lead.name}, {percentLabel}{t(". Apri la sezione Download", $language)}"
      onclick={() => selectSection("downloads")}
      class="relative flex size-12 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-white/5 text-zinc-400 transition-colors duration-200 hover:bg-white/10 hover:text-white light:bg-zinc-900/5 light:text-zinc-500 light:hover:bg-zinc-900/10 light:hover:text-zinc-900"
    >
      <span
        class="absolute inset-x-0 bottom-0 bg-legio-download"
        style="height: {percent}%"
        aria-hidden="true"
      ></span>
      <span class="relative">
        <Icon name="downloads" size="h-5 w-5" />
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
