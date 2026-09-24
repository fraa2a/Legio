<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    expanded,
    active = false,
    onClick,
    ariaLabel,
    children,
  }: {
    label?: string;
    expanded: boolean;
    active?: boolean;
    onClick?: () => void;
    ariaLabel?: string;
    children?: Snippet;
  } = $props();

  const accessibleName = $derived(ariaLabel ?? (expanded || !label ? undefined : label));

  const pillClass = $derived.by(() => {
    if (expanded) return active ? "w-50 bg-white/10" : "w-50 hover:bg-white/10";
    return active ? "w-12 bg-white/10" : "w-12 hover:bg-white/10";
  });
</script>

<button
  type="button"
  aria-label={accessibleName}
  aria-pressed={active}
  class="flex w-full items-center transition-colors duration-200 {active
    ? "text-white"
    : "text-zinc-400 hover:text-white"}"
  onclick={onClick}
>
  <span
    class="flex h-12 items-center gap-2 overflow-hidden rounded-lg transition-[width,background-color,color] duration-300 ease-out {pillClass}"
  >
    {#if children}
      <span class="ml-2 flex size-8 shrink-0 items-center justify-center">{@render children()}</span>
    {/if}
    {#if label !== undefined}
      <span class="overflow-hidden whitespace-nowrap text-sm font-medium">
        {label}
      </span>
    {/if}
  </span>
</button>