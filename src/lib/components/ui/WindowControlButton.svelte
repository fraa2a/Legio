<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    label,
    onClick,
    variant = "default",
    children,
  }: {
    label: string;
    onClick: () => void;
    variant?: "default" | "danger";
    children: Snippet;
  } = $props();

  const hoverClass = $derived(
    variant === "danger"
      ? "hover:bg-[#e81123] hover:text-white"
      : "hover:bg-white/10 hover:text-white light:hover:bg-zinc-900/10 light:hover:text-zinc-900",
  );
</script>

<button
  type="button"
  aria-label={label}
  class="flex size-8 items-center justify-center rounded-full text-zinc-300 light:text-zinc-600 {hoverClass}"
  onclick={onClick}
>
  {@render children()}
</button>