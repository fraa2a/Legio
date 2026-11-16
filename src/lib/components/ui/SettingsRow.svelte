<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    description,
    children,
  }: {
    label: string;
    description?: string;
    children: Snippet;
  } = $props();
</script>

<div class="flex items-center justify-between gap-6">
  <div class="min-w-0">
    <p class="text-sm font-medium text-legio-fg">{label}</p>
    {#if description}
      <p class="mt-0.5 break-words text-xs text-legio-fg-muted">{description}</p>
    {/if}
  </div>
  <div class="shrink-0">{@render children()}</div>
</div>