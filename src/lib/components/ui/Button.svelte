<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    variant = "primary",
    type = "button",
    disabled = false,
    pressed,
    circle = false,
    square = false,
    onClick,
    class: className = "",
    children,
  }: {
    label: string;
    variant?: "primary" | "secondary" | "danger" | "download" | "play" | "launching" | "running";
    type?: "button" | "submit";
    disabled?: boolean;
    pressed?: boolean;
    circle?: boolean;
    square?: boolean;
    onClick?: () => void;
    class?: string;
    children?: Snippet;
  } = $props();

  const layoutClass = $derived(
    circle || square
      ? `inline-flex size-11 items-center justify-center ${circle ? "rounded-full" : "rounded-[10px]"} text-sm font-medium transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50`
      : "inline-flex h-10 items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50",
  );

  const variantClass = $derived.by(() => {
    if (variant === "download") return "bg-legio-download text-white hover:bg-legio-download-hover disabled:hover:bg-legio-download";
    if (variant === "play") return "bg-legio-play text-white hover:bg-legio-play-hover disabled:hover:bg-legio-play";
    if (variant === "launching") return "bg-legio-launching text-white hover:bg-legio-launching-hover disabled:hover:bg-legio-launching";
    if (variant === "running") return "bg-legio-running text-white hover:bg-legio-running-hover disabled:hover:bg-legio-running";
    if (variant === "danger") return "bg-[#e81123] text-white hover:bg-[#c50e1e] disabled:hover:bg-[#e81123]";
    if (variant === "secondary") {
      return "bg-white/10 text-zinc-100 hover:bg-white/20 light:bg-zinc-200 light:text-zinc-900 light:hover:bg-zinc-300 disabled:hover:bg-zinc-200";
    }
    return "bg-legio-accent text-legio-accent-text hover:brightness-90 disabled:hover:brightness-100";
  });
</script>

<button
  {type}
  {disabled}
  aria-label={circle || square ? label : undefined}
  aria-pressed={pressed}
  onclick={onClick}
  class="{layoutClass} {variantClass} {className}"
>
  {@render children?.()}
  {#if !circle && !square}{label}{/if}
</button>
