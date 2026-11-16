<script lang="ts">
  import type { ChartBar } from "./recap-model";

  let {
    title,
    bars,
    labelVisible = () => true,
  }: {
    title: string;
    bars: ChartBar[];
    labelVisible?: (index: number) => boolean;
  } = $props();
</script>

<!-- Each bar carries its own text alternative; the visible labels are decorative. -->
<div class="flex h-44 items-stretch gap-1" role="group" aria-label={title}>
  {#each bars as bar, index (index)}
    <div class="flex min-w-0 flex-1 flex-col items-center gap-1.5" role="img" aria-label={bar.text} title={bar.text}>
      <div class="flex w-full flex-1 items-end">
        <div class="w-full rounded-t-control bg-legio-accent" style:height="{bar.percent}%"></div>
      </div>
      <span class="h-4 w-full truncate text-center text-xs text-legio-fg-muted" aria-hidden="true">{labelVisible(index) ? bar.label : ""}</span>
    </div>
  {/each}
</div>
