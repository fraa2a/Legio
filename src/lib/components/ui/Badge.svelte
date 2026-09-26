<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    tone = "neutral",
    title,
    children,
  }: {
    tone?: "neutral" | "info" | "success" | "warning" | "danger";
    title: string;
    children?: Snippet;
  } = $props();

  const toneClass = $derived.by(() => {
    if (tone === "success") return "bg-emerald-500/15 text-emerald-300 light:text-emerald-800";
    if (tone === "warning") return "bg-amber-500/15 text-amber-300 light:text-amber-800";
    if (tone === "danger") return "bg-red-500/15 text-red-300 light:text-red-700";
    if (tone === "info") return "bg-sky-500/15 text-sky-300 light:text-sky-800";
    return "bg-white/10 text-zinc-300 light:bg-zinc-200 light:text-zinc-700";
  });
</script>

<span
  class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium {toneClass}"
>
  {#if children}{@render children()}{/if}
  {title}
</span>
