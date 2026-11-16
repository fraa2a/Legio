<script lang="ts">
  import type { Snippet } from "svelte";
  import Tooltip from "./Tooltip.svelte";

  let {
    label,
    title,
    size = "sm",
    variant = "ghost",
    pressed,
    disabled = false,
    onClick,
    class: className = "",
    children,
  }: {
    label: string;
    title?: string;
    // Inline sizes only sit inside other controls, such as a tab or a banner.
    size?: "inline-sm" | "inline-md" | "sm" | "md";
    variant?: "ghost" | "soft" | "plain" | "danger";
    pressed?: boolean;
    disabled?: boolean;
    onClick: () => void;
    class?: string;
    children: Snippet;
  } = $props();

  const sizeClass = $derived.by(() => {
    if (size === "inline-sm") return "size-5";
    if (size === "inline-md") return "size-7";
    if (size === "md") return "size-control-md";
    return "size-control-sm";
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
  class="group inline-flex shrink-0 items-center justify-center rounded-control transition-colors disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent {sizeClass} {variantClass} {className}"
>
  {@render children()}
</button>
{/snippet}

{#if title}
  <Tooltip text={title}>{@render control()}</Tooltip>
{:else}
  {@render control()}
{/if}
