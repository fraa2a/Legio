<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    expanded,
    active = false,
    togglesSidebar = false,
    count,
    onClick,
    ariaLabel,
    children,
  }: {
    label?: string;
    expanded: boolean;
    active?: boolean;
    togglesSidebar?: boolean;
    count?: number;
    onClick?: () => void;
    ariaLabel?: string;
    children?: Snippet;
  } = $props();

  const accessibleName = $derived(ariaLabel ?? (expanded || !label ? undefined : label));

  const pillClass = $derived(
    active
      ? "w-full bg-white/10 light:bg-zinc-900/10"
      : "w-full hover:bg-white/10 light:hover:bg-zinc-900/5",
  );
</script>

<button
  type="button"
  aria-label={accessibleName}
  aria-current={active ? "page" : undefined}
  aria-expanded={togglesSidebar ? expanded : undefined}
  class="flex w-full items-center {active
    ? "text-white light:text-zinc-900"
    : "text-zinc-400 hover:text-white light:text-zinc-500 light:hover:text-zinc-900"}"
  onclick={onClick}
>
  <span
    class="flex h-12 items-center gap-2 overflow-hidden rounded-lg transition-colors duration-200 {pillClass}"
  >
    {#if children}
      <span class="ml-2 flex size-8 shrink-0 items-center justify-center">{@render children()}</span>
    {/if}
    {#if label !== undefined}
      <span class="overflow-hidden whitespace-nowrap text-sm font-medium">
        {label}
      </span>
    {/if}
    {#if count !== undefined && count > 0 && expanded}
      <span
        class="ml-auto mr-2 rounded-full bg-white/15 px-1.5 py-0.5 text-xs leading-4 font-semibold text-white light:bg-zinc-900/15 light:text-zinc-900"
        aria-hidden="true"
      >
        {count}
      </span>
    {/if}
  </span>
</button>
