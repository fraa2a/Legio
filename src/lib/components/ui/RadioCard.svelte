<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    name,
    value,
    checked,
    disabled = false,
    variant = "row",
    onSelect,
    class: className = "",
    children,
  }: {
    name: string;
    value?: string;
    checked: boolean;
    disabled?: boolean;
    // Rows show the radio; tiles mark the choice with an accent border instead.
    variant?: "row" | "tile";
    onSelect: () => void;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<label
  class="flex cursor-pointer text-sm text-legio-fg transition-colors has-disabled:cursor-not-allowed has-disabled:opacity-50 {variant === 'tile'
    ? 'flex-col gap-2 rounded-card border border-legio-line-subtle p-2 not-has-checked:hover:bg-legio-fill has-checked:border-legio-accent has-checked:bg-legio-accent/10 has-focus-visible:outline-2 has-focus-visible:outline-offset-2 has-focus-visible:outline-(--legio-focus)'
    : 'items-center gap-3 rounded-control px-3 py-2 hover:bg-legio-fill has-checked:bg-legio-fill-hover'} {className}"
>
  <input
    type="radio"
    class={variant === "tile" ? "sr-only" : "size-4 shrink-0"}
    {name}
    {value}
    {checked}
    {disabled}
    onchange={onSelect}
  />
  {@render children()}
</label>
