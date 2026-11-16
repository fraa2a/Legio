<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    tone = "neutral",
    title,
    children,
  }: {
    tone?: "neutral" | "info" | "success" | "warning" | "danger" | "tier-platinum" | "tier-gold" | "tier-silver" | "tier-bronze" | "tier-borked";
    title: string;
    children?: Snippet;
  } = $props();

  const toneClass = $derived.by(() => {
    if (tone === "tier-platinum") return "bg-legio-tier-platinum/15 text-legio-tier-platinum";
    if (tone === "tier-gold") return "bg-legio-tier-gold/15 text-legio-tier-gold";
    if (tone === "tier-silver") return "bg-legio-tier-silver/15 text-legio-tier-silver";
    if (tone === "tier-bronze") return "bg-legio-tier-bronze/15 text-legio-tier-bronze";
    if (tone === "tier-borked") return "bg-legio-tier-borked/15 text-legio-tier-borked";
    if (tone === "success") return "bg-legio-success/15 text-legio-success";
    if (tone === "warning") return "bg-legio-warning/15 text-legio-warning";
    if (tone === "danger") return "bg-legio-error/15 text-legio-error";
    if (tone === "info") return "bg-legio-info/15 text-legio-info";
    return "bg-legio-fill-hover text-legio-fg";
  });
</script>

<span
  class="inline-flex max-w-full items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium {toneClass}"
>
  {#if children}{@render children()}{/if}
  <span class="truncate">{title}</span>
</span>
