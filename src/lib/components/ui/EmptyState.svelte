<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    icon,
    title,
    description,
    children,
  }: {
    icon?: IconName;
    title?: string;
    description?: string;
    children?: Snippet;
  } = $props();
</script>

<div class="flex flex-col items-center gap-3 rounded-panel legio-glass p-inset-wide text-center">
  {#if icon}
    <span class="grid size-12 place-items-center rounded-full bg-legio-fill-hover text-legio-fg"><Icon name={icon} size="xl" /></span>
  {/if}
  {#if title}<h2 class="text-lg font-semibold text-legio-fg">{title}</h2>{/if}
  {#if description}<p class="max-w-md text-sm text-legio-fg-muted">{description}</p>{/if}
  {#if children}<div class="mt-1">{@render children()}</div>{/if}
</div>
