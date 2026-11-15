<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  let { categories, selected, label, onSelect, footer }: {
    categories: { id: string; label: string; icon: IconName }[];
    selected: string;
    label: string;
    onSelect: (id: string) => void;
    footer?: Snippet;
  } = $props();
</script>

<nav class="flex w-52 shrink-0 flex-col gap-1 overflow-y-auto border-r border-legio-line p-3" aria-label={label}>
  {#each categories as category (category.id)}
    <button type="button" aria-current={selected === category.id ? "page" : undefined}
      class="flex items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors {selected === category.id ? 'bg-legio-fill-hover text-legio-fg' : 'text-legio-fg-muted hover:bg-legio-fill hover:text-legio-fg'}"
      onclick={() => onSelect(category.id)}>
      <Icon name={category.icon} size="size-4 shrink-0" />{category.label}
    </button>
  {/each}
  {#if footer}<div class="mt-auto pt-3">{@render footer()}</div>{/if}
</nav>
