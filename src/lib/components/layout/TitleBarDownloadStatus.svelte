<script lang="ts">
  import { t, language } from "../../i18n";
  import { activeDownloads } from "../../stores/downloads";
  import { selectSection } from "../../stores/navigation";
  import { formatBytes } from "../../utils/format";
  import Icon from "../ui/Icon.svelte";
  import { summarizeDownloads } from "./title-bar-model";

  const summary = $derived(summarizeDownloads($activeDownloads));
  const speed = $derived(summary === null || summary.speedBps === 0 ? null : `${formatBytes(summary.speedBps, $language)}/s`);
  const label = $derived(summary === null ? "" : t("Download attivi: {0}, {1}%. Apri la sezione Download", $language, [summary.count, summary.percent]));
</script>

<!-- Below 72rem of title bar only the icon and the count remain; the label still carries the progress. -->
{#if summary !== null}
  <button
    type="button"
    aria-label={label}
    title={speed === null ? label : `${label} (${speed})`}
    onclick={() => selectSection("downloads")}
    class="legio-glass relative flex h-control-md shrink-0 items-center gap-2 overflow-hidden rounded-card px-3 text-xs font-medium text-legio-fg tabular-nums transition-colors hover:bg-legio-fill-hover"
  >
    <span class="text-legio-fg-muted"><Icon name="downloads" /></span>
    <span>{summary.count}</span>
    <span class="hidden @min-[72rem]:inline">{summary.percent}%</span>
    {#if speed !== null}<span class="hidden text-legio-fg-muted @min-[72rem]:inline">{speed}</span>{/if}
    <span class="absolute inset-x-0 bottom-0 h-0.5 bg-legio-fill-hover" aria-hidden="true">
      <span class="block h-full bg-legio-download transition-[width] duration-(--legio-motion-slow) motion-reduce:transition-none" style:width="{summary.percent}%"></span>
    </span>
  </button>
{/if}
