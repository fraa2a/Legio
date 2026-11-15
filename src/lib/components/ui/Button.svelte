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
    variant?: "primary" | "secondary" | "overlay" | "danger" | "danger-soft" | "download" | "play" | "launching" | "running";
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
    if (variant === "running" || variant === "danger") return "bg-legio-danger text-white hover:bg-legio-danger-hover disabled:hover:bg-legio-danger";
    if (variant === "danger-soft") return "bg-legio-error/15 text-legio-error hover:bg-legio-error/25 disabled:hover:bg-legio-error/15";
    if (variant === "overlay") {
      return "border border-legio-line-subtle bg-zinc-950/60 text-legio-fg backdrop-blur-md hover:bg-zinc-950/75 light:bg-zinc-100/70 light:hover:bg-zinc-200";
    }
    if (variant === "secondary") {
      return "bg-legio-fill-hover text-legio-fg hover:bg-legio-fill-active disabled:hover:bg-legio-fill-hover";
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
