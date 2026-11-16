<script lang="ts">
  import { language, t } from "../../i18n";
  import type { LibraryUsage } from "../../services/disk-usage";
  import { formatBytes } from "../../utils/format";
  import { usageSegments } from "./disk-usage-model";

  let { usage }: { usage: LibraryUsage } = $props();

  const segments = $derived(usageSegments(usage));
  const legio = $derived(formatBytes(usage.legioBytes, $language));
  const other = $derived(formatBytes(usage.otherBytes ?? 0, $language));
  const free = $derived(formatBytes(usage.freeBytes ?? 0, $language));
  const summary = $derived(t("Giochi Legio {0}, altro {1}, liberi {2}", $language, [legio, other, free]));
</script>

{#if segments !== null}
  <div class="flex flex-col gap-1.5">
    <span class="flex h-1.5 w-full overflow-hidden rounded-full bg-legio-fill-hover" role="img" aria-label={summary}>
      <span class="h-full bg-legio-download transition-[width] duration-(--legio-motion-slow) motion-reduce:transition-none" style:width={`${segments.legio}%`}></span>
      <span class="h-full bg-legio-fg-subtle transition-[width] duration-(--legio-motion-slow) motion-reduce:transition-none" style:width={`${segments.other}%`}></span>
    </span>
    <p class="flex flex-wrap gap-x-3 text-xs text-legio-fg-muted">
      <span class="inline-flex items-center gap-1.5"><span class="size-2 rounded-full bg-legio-download" aria-hidden="true"></span>{t("Giochi Legio: {0}", $language, [legio])}{usage.estimated ? ` (${t("stima", $language)})` : ""}</span>
      <span class="inline-flex items-center gap-1.5"><span class="size-2 rounded-full bg-legio-fg-subtle" aria-hidden="true"></span>{t("Altro: {0}", $language, [other])}</span>
      <span class="inline-flex items-center gap-1.5"><span class="size-2 rounded-full bg-legio-fill-hover" aria-hidden="true"></span>{t("Liberi: {0}", $language, [free])}</span>
    </p>
  </div>
{/if}
