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
      ? "w-full bg-legio-fill-hover"
      : "w-full hover:bg-legio-fill-hover",
  );
</script>

<button
  type="button"
  aria-label={accessibleName}
  aria-current={active ? "page" : undefined}
  aria-expanded={togglesSidebar ? expanded : undefined}
  class="flex w-full items-center {active
    ? "text-legio-fg"
    : "text-legio-fg-muted hover:text-legio-fg"}"
  onclick={() => onClick?.()}
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
        class="ml-auto mr-2 rounded-full bg-legio-fill-active px-1.5 py-0.5 text-xs leading-4 font-semibold text-legio-fg"
        aria-hidden="true"
      >
        {count}
      </span>
    {/if}
  </span>
</button>
