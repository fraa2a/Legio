<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    actions,
    children,
    class: className = "",
  }: {
    title: string;
    actions?: Snippet;
    children: Snippet;
    class?: string;
  } = $props();
</script>

<section class="flex flex-col gap-4 rounded-2xl bg-white/5 p-5 light:bg-zinc-100 {className}">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <h3 class="text-sm font-semibold text-zinc-200 light:text-zinc-800">{title}</h3>
    {#if actions}{@render actions()}{/if}
  </div>
  {@render children()}
</section>
