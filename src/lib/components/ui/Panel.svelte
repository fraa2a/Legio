<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    description,
    actions,
    children,
    class: className = "",
  }: {
    title: string;
    description?: string;
    actions?: Snippet;
    children: Snippet;
    class?: string;
  } = $props();

  const titleId = $props.id();
</script>

<section class="flex flex-col gap-rhythm-row rounded-panel legio-glass p-inset-wide {className}" aria-labelledby={titleId}>
  <div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
    <div class="min-w-0">
      <h2 id={titleId} class="text-base font-semibold text-legio-fg">{title}</h2>
      {#if description}<p class="mt-0.5 text-sm text-legio-fg-muted">{description}</p>{/if}
    </div>
    {#if actions}{@render actions()}{/if}
  </div>
  {@render children()}
</section>
