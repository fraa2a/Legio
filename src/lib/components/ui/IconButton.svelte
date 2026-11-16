<script lang="ts">
  import type { Snippet } from "svelte";
  import Tooltip from "./Tooltip.svelte";

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
      return "bg-legio-fill-hover text-legio-fg hover:bg-legio-fill-active hover:text-legio-fg";
    }
    if (variant === "plain") return "hover:bg-legio-fill-active";
    if (variant === "danger") {
      return "text-legio-fg-muted hover:bg-legio-danger hover:text-white";
    }
    if (pressed) return "bg-legio-fill-hover text-legio-fg hover:bg-legio-fill-active";
    return "text-legio-fg-muted hover:bg-legio-fill-hover hover:text-legio-fg";
  });
</script>

{#snippet control()}
<button
  type="button"
  aria-label={label}
  aria-pressed={pressed}
  {disabled}
  onclick={onClick}
  class="group inline-flex shrink-0 items-center justify-center transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent {sizeClass} {variantClass} {className}"
>
  {@render children()}
</button>
{/snippet}

{#if title}
  <Tooltip text={title}>{@render control()}</Tooltip>
{:else}
  {@render control()}
{/if}
