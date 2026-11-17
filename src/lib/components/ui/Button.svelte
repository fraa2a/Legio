<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    variant = "primary",
    size = "md",
    type = "button",
    disabled = false,
    pressed,
    shape = "rect",
    iconOnly = false,
    onClick,
    class: className = "",
    children,
  }: {
    label: string;
    variant?: "primary" | "secondary" | "ghost" | "link" | "overlay" | "danger" | "danger-soft" | "download" | "play" | "launching" | "running";
    size?: "sm" | "md" | "hero";
    type?: "button" | "submit";
    disabled?: boolean;
    pressed?: boolean;
    // Rect sits on panels; pill and circle stand out over artwork.
    shape?: "rect" | "pill" | "circle";
    iconOnly?: boolean;
    onClick?: () => void;
    class?: string;
    children?: Snippet;
  } = $props();

  // A circle has no room for a label.
  const icon = $derived(iconOnly || shape === "circle");

  const sizeClass = $derived.by(() => {
    if (variant === "link") return size === "sm" ? "gap-1.5 text-xs font-medium" : "gap-1.5 text-sm font-medium";
    if (size === "sm") return icon ? "size-control-sm" : "h-control-sm gap-1.5 px-3 text-xs font-medium";
    if (size === "hero") return icon ? "size-control-hero" : "h-control-hero gap-2 px-4 text-xl font-bold";
    return icon ? "size-control-md" : "h-control-md gap-2 px-4 text-sm font-medium";
  });

  const shapeClass = $derived(shape === "rect" ? (size === "hero" ? "rounded-card" : "rounded-control") : "rounded-full");

  const variantClass = $derived.by(() => {
    if (variant === "download") return "bg-legio-download text-white hover:bg-legio-download-hover disabled:hover:bg-legio-download";
    if (variant === "play") return "bg-legio-play text-white hover:bg-legio-play-hover disabled:hover:bg-legio-play";
    if (variant === "launching") return "bg-legio-launching text-white hover:bg-legio-launching-hover disabled:hover:bg-legio-launching";
    if (variant === "running" || variant === "danger") return "bg-legio-danger text-white hover:bg-legio-danger-hover disabled:hover:bg-legio-danger";
    if (variant === "danger-soft") return "bg-legio-error/15 text-legio-error hover:bg-legio-error/25 disabled:hover:bg-legio-error/15";
    if (variant === "overlay") {
      return "legio-glass border border-legio-line-subtle text-legio-fg hover:bg-legio-fill-hover";
    }
    if (variant === "ghost") return "text-legio-fg hover:bg-legio-fill-hover disabled:hover:bg-transparent";
    if (variant === "link") return "text-legio-fg underline-offset-2 hover:underline disabled:hover:no-underline";
    if (variant === "secondary") {
      return "bg-legio-fill-hover text-legio-fg hover:bg-legio-fill-active disabled:hover:bg-legio-fill-hover";
    }
    return "bg-legio-accent text-legio-accent-text hover:brightness-90 disabled:hover:brightness-100";
  });
</script>

<button
  {type}
  {disabled}
  aria-label={icon ? label : undefined}
  aria-pressed={pressed}
  onclick={onClick}
  class="inline-flex items-center justify-center transition-colors disabled:cursor-not-allowed disabled:opacity-50 {sizeClass} {shapeClass} {variantClass} {className}"
>
  {@render children?.()}
  {#if !icon}{label}{/if}
</button>
