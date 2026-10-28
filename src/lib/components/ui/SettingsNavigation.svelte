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

<nav class="flex w-52 shrink-0 flex-col gap-1 overflow-y-auto border-r border-white/15 p-3 light:border-zinc-900/15" aria-label={label}>
  {#each categories as category (category.id)}
    <button type="button" aria-current={selected === category.id ? "page" : undefined}
      class="flex items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm font-medium transition-colors {selected === category.id ? 'bg-white/10 text-white light:bg-zinc-900/10 light:text-zinc-900' : 'text-zinc-400 hover:bg-white/5 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/5 light:hover:text-zinc-900'}"
      onclick={() => onSelect(category.id)}>
      <Icon name={category.icon} size="size-4 shrink-0" />{category.label}
    </button>
  {/each}
  {#if footer}<div class="mt-auto pt-3">{@render footer()}</div>{/if}
</nav>
