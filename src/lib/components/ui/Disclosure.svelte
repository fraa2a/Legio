<script lang="ts">
  import type { Snippet } from "svelte";
  import { slide } from "svelte/transition";
  import { reducedMotion } from "../../utils/motion";
  import Icon from "./Icon.svelte";

  let { label, children }: { label: string; children: Snippet } = $props();

  let open = $state(false);
</script>

<div class="mt-1">
  <button
    type="button"
    class="flex w-fit cursor-pointer items-center gap-1 text-xs text-legio-fg-muted transition-colors hover:text-legio-fg"
    aria-expanded={open}
    onclick={() => { open = !open; }}
  >
    {label}
    <span class="transition-transform motion-reduce:transition-none {open ? 'rotate-180' : ''}"><Icon name="chevron-down" size="sm" /></span>
  </button>
  {#if open}
    <div transition:slide={{ duration: reducedMotion ? 0 : 200 }}>{@render children()}</div>
  {/if}
</div>
