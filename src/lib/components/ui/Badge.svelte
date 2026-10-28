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
    if (tone === "success") return "bg-legio-success/15 text-legio-success";
    if (tone === "warning") return "bg-legio-warning/15 text-legio-warning";
    if (tone === "danger") return "bg-legio-error/15 text-legio-error";
    if (tone === "info") return "bg-legio-info/15 text-legio-info";
    return "bg-white/10 text-zinc-300 light:bg-zinc-900/10 light:text-zinc-700";
  });
</script>

<span
  class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium {toneClass}"
>
  {#if children}{@render children()}{/if}
  {title}
</span>
