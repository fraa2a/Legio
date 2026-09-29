<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    description,
    children,
  }: {
    title?: string;
    description?: string;
    children: Snippet;
  } = $props();
</script>

<section class="rounded-xl bg-white/5 p-5 light:bg-zinc-100">
  {#if title}
    <h3 class="text-sm font-semibold text-zinc-100 light:text-zinc-900">{title}</h3>
  {/if}
  {#if description}
    <p class="mt-1 text-xs text-zinc-500 light:text-zinc-600">{description}</p>
  {/if}
  <div class="flex flex-col gap-4 {title || description ? 'mt-4' : ''}">
    {@render children()}
  </div>
</section>