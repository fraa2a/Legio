<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    variant = "primary",
    size = "md",
    type = "button",
    disabled = false,
    pressed,
    circle = false,
    square = false,
    pill = false,
    onClick,
    class: className = "",
    children,
  }: {
    label: string;
    variant?: "primary" | "secondary" | "overlay" | "danger" | "download" | "play" | "launching" | "running";
    size?: "sm" | "md" | "hero";
    type?: "button" | "submit";
    disabled?: boolean;
    pressed?: boolean;
    circle?: boolean;
    square?: boolean;
    pill?: boolean;
    onClick?: () => void;
    class?: string;
    children?: Snippet;
  } = $props();

  const iconOnly = $derived(circle || square);

  const sizeClass = $derived.by(() => {
    if (size === "sm") return iconOnly ? "size-8" : "h-8 gap-1.5 px-3 text-xs font-medium";
    if (size === "hero") return iconOnly ? "size-15" : "h-15 gap-2 px-4 text-xl font-bold";
    return iconOnly ? "size-11" : "h-10 gap-2 px-4 text-sm font-medium";
  });

  const shapeClass = $derived(circle || pill ? "rounded-full" : size === "hero" ? "rounded-xl" : "rounded-lg");

  const variantClass = $derived.by(() => {
    if (variant === "download") return "bg-legio-download text-white hover:bg-legio-download-hover disabled:hover:bg-legio-download";
    if (variant === "play") return "bg-legio-play text-white hover:bg-legio-play-hover disabled:hover:bg-legio-play";
    if (variant === "launching") return "bg-legio-launching text-white hover:bg-legio-launching-hover disabled:hover:bg-legio-launching";
    if (variant === "running") return "bg-legio-running text-white hover:bg-legio-running-hover disabled:hover:bg-legio-running";
    if (variant === "danger") return "bg-legio-danger text-white hover:bg-legio-danger-hover disabled:hover:bg-legio-danger";
    if (variant === "overlay") {
      return "border border-white/10 bg-zinc-950/60 text-zinc-100 backdrop-blur-md hover:bg-zinc-950/75 light:border-zinc-900/10 light:bg-zinc-100/70 light:text-zinc-900 light:hover:bg-zinc-200";
    }
    if (variant === "secondary") {
      return "bg-white/10 text-zinc-100 hover:bg-white/20 light:bg-zinc-200 light:text-zinc-900 light:hover:bg-zinc-300 disabled:hover:bg-white/10 light:disabled:hover:bg-zinc-200";
    }
    return "bg-legio-accent text-legio-accent-text hover:brightness-90 disabled:hover:brightness-100";
  });
</script>

<button
  {type}
  {disabled}
  aria-label={iconOnly ? label : undefined}
  aria-pressed={pressed}
  onclick={onClick}
  class="inline-flex items-center justify-center transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-50 {sizeClass} {shapeClass} {variantClass} {className}"
>
  {@render children?.()}
  {#if !iconOnly}{label}{/if}
</button>
