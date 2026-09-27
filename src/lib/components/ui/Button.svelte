<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    variant = "primary",
    type = "button",
    disabled = false,
    pressed,
    circle = false,
    onClick,
    class: className = "",
    children,
  }: {
    label: string;
    variant?: "primary" | "secondary" | "danger";
    type?: "button" | "submit";
    disabled?: boolean;
    pressed?: boolean;
    circle?: boolean;
    onClick?: () => void;
    class?: string;
    children?: Snippet;
  } = $props();

  const layoutClass = $derived(
    circle
      ? "inline-flex size-11 items-center justify-center rounded-full text-sm font-medium transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50"
      : "inline-flex h-10 items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50",
  );

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
  aria-label={circle ? label : undefined}
  aria-pressed={pressed}
  onclick={onClick}
  class="{layoutClass} {variantClass} {className}"
>
  {@render children?.()}
  {#if !circle}{label}{/if}
</button>
