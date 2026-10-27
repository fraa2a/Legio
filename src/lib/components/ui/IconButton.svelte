<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    title,
    size = "md",
    variant = "ghost",
    pressed,
    disabled = false,
    onClick,
    class: className = "",
    children,
  }: {
    label: string;
    title?: string;
    size?: "xs" | "sm" | "md" | "lg";
    variant?: "ghost" | "soft" | "plain" | "danger";
    pressed?: boolean;
    disabled?: boolean;
    onClick: () => void;
    class?: string;
    children: Snippet;
  } = $props();

  const sizeClass = $derived.by(() => {
    if (size === "xs") return "size-5 rounded";
    if (size === "sm") return "size-7 rounded-md";
    if (size === "lg") return "size-9 rounded-lg";
    return "size-8 rounded-lg";
  });

  const variantClass = $derived.by(() => {
    if (variant === "soft") {
      return "bg-white/10 text-zinc-200 hover:bg-white/25 hover:text-white light:bg-zinc-900/10 light:text-zinc-700 light:hover:bg-zinc-900/20 light:hover:text-zinc-900";
    }
    if (variant === "plain") return "hover:bg-white/15 light:hover:bg-zinc-900/10";
    if (variant === "danger") {
      return "text-zinc-300 hover:bg-legio-danger hover:text-white light:text-zinc-600 light:hover:text-white";
    }
    if (pressed) return "bg-white/10 text-white hover:bg-white/15 light:bg-zinc-900/10 light:text-zinc-900 light:hover:bg-zinc-900/15";
    return "text-zinc-400 hover:bg-white/10 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/10 light:hover:text-zinc-900";
  });
</script>

<button
  type="button"
  aria-label={label}
  aria-pressed={pressed}
  {title}
  {disabled}
  onclick={onClick}
  class="group inline-flex shrink-0 items-center justify-center transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent {sizeClass} {variantClass} {className}"
>
  {@render children()}
</button>
