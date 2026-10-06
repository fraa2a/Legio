<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    title,
    icon,
    collapsible = false,
    children,
  }: {
    title: string;
    icon?: IconName;
    collapsible?: boolean;
    children: Snippet;
  } = $props();

  let open = $state(false);
</script>

<section
  class="flex flex-col gap-4 rounded-2xl legio-glass p-5 shadow-sm shadow-black/20 light:shadow-zinc-900/5"
>
  {#snippet iconBadge()}
    {#if icon}
      <span
        class="grid size-8 shrink-0 place-items-center rounded-lg bg-white/10 text-zinc-200 light:bg-zinc-900/10 light:text-zinc-800"
      >
        <Icon name={icon} size="size-4" />
      </span>
    {/if}
  {/snippet}
  {#if collapsible}
    <h3 class="text-sm font-semibold text-zinc-100 light:text-zinc-900">
      <button
        type="button"
        class="group flex w-full cursor-pointer items-center gap-3 text-left"
        aria-expanded={open}
        onclick={() => { open = !open; }}
      >
        <span
          class="flex min-w-0 items-center gap-3 rounded-lg transition-colors hover:bg-white/5 group-focus-visible:outline-2 group-focus-visible:outline-legio-accent light:hover:bg-zinc-900/5"
        >
          {@render iconBadge()}
          <span class="min-w-0">{title}</span>
        </span>
        <span
          class="ml-auto shrink-0 text-zinc-500 transition-transform motion-reduce:transition-none {open ? 'rotate-180' : ''}"
          aria-hidden="true"
        >
          <Icon name="chevron-down" size="size-4" />
        </span>
      </button>
    </h3>
  {:else}
    <div class="flex items-center gap-3">
      {@render iconBadge()}
      <div class="min-w-0">
        <h3 class="text-sm font-semibold text-zinc-100 light:text-zinc-900">{title}</h3>
      </div>
    </div>
  {/if}
  {#if !collapsible || open}
    <div class="flex flex-col gap-4">
      {@render children()}
    </div>
  {/if}
</section>
