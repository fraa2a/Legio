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

  const pillClass = $derived.by(() => {
    if (expanded) {
      return active
        ? "w-50 bg-white/10 light:bg-zinc-900/10"
        : "w-50 hover:bg-white/10 light:hover:bg-zinc-900/5";
    }
    return active
      ? "w-12 bg-white/10 light:bg-zinc-900/10"
      : "w-12 hover:bg-white/10 light:hover:bg-zinc-900/5";
  });
</script>

<button
  type="button"
  aria-label={accessibleName}
  aria-current={active ? "page" : undefined}
  aria-expanded={togglesSidebar ? expanded : undefined}
  class="flex w-full items-center transition-colors duration-200 {active
    ? "text-white light:text-zinc-900"
    : "text-zinc-400 hover:text-white light:text-zinc-500 light:hover:text-zinc-900"}"
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
    {#if count !== undefined && count > 0 && expanded}
      <span
        class="ml-auto rounded-full bg-white/15 px-2 py-0.5 text-xs font-semibold text-white light:bg-zinc-900/15 light:text-zinc-900"
        aria-hidden="true"
      >
        {count}
      </span>
    {/if}
  </span>
</button>
