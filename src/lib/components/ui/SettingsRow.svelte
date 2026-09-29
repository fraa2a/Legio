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
    <p class="text-sm text-zinc-100 light:text-zinc-900">{label}</p>
    {#if description}
      <p class="mt-0.5 break-words text-xs text-zinc-500 light:text-zinc-600">{description}</p>
    {/if}
  </div>
  <div class="shrink-0">{@render children()}</div>
</div>