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

<div class="flex flex-col items-center gap-3 rounded-2xl legio-glass px-6 py-10 text-center">
  {#if icon}
    <span class="grid size-12 place-items-center rounded-full bg-legio-fill-hover text-legio-fg"><Icon name={icon} size="size-6" /></span>
  {/if}
  {#if title}<h2 class="text-lg font-medium text-legio-fg">{title}</h2>{/if}
  {#if description}<p class="max-w-md text-sm text-legio-fg-muted">{description}</p>{/if}
  {#if children}<div class="mt-1">{@render children()}</div>{/if}
</div>
