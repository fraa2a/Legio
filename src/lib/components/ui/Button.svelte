<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    variant = "primary",
    type = "button",
    disabled = false,
    onClick,
    children,
  }: {
    label: string;
    variant?: "primary" | "secondary" | "danger";
    type?: "button" | "submit";
    disabled?: boolean;
    onClick?: () => void;
    children?: Snippet;
  } = $props();

  const variantClass = $derived.by(() => {
    if (variant === "danger") return "bg-[#e81123] text-white hover:bg-[#c50e1e] disabled:hover:bg-[#e81123]";
    if (variant === "secondary") {
      return "bg-white/10 text-zinc-100 hover:bg-white/20 light:bg-zinc-200 light:text-zinc-900 light:hover:bg-zinc-300 disabled:hover:bg-zinc-200";
    }
    return "bg-white text-zinc-900 hover:bg-zinc-200 disabled:hover:bg-white";
  });
</script>

<button
  {type}
  {disabled}
  onclick={onClick}
  class="inline-flex items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50 {variantClass}"
>
  {@render children?.()}
  {label}
</button>
