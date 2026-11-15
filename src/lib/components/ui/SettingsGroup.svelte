<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    title,
    icon,
    collapsible = false,
    defaultOpen = false,
    children,
  }: {
    title: string;
    icon?: IconName;
    collapsible?: boolean;
    defaultOpen?: boolean;
    children: Snippet;
  } = $props();

  let open = $state(untrack(() => defaultOpen));
</script>

<section class="flex flex-col gap-4 rounded-2xl legio-glass p-5">
  {#snippet iconBadge()}
    {#if icon}
      <span
        class="grid size-8 shrink-0 place-items-center rounded-lg bg-legio-fill-hover text-legio-fg"
      >
        <Icon name={icon} size="size-4" />
      </span>
    {/if}
  {/snippet}
  {#if collapsible}
    <h3 class="text-sm font-semibold text-legio-fg">
      <button
        type="button"
        class="group flex w-full cursor-pointer items-center gap-3 text-left"
        aria-expanded={open}
        onclick={() => { open = !open; }}
      >
        <span
          class="flex min-w-0 items-center gap-3 rounded-lg transition-colors hover:bg-legio-fill"
        >
          {@render iconBadge()}
          <span class="min-w-0">{title}</span>
        </span>
        <span
          class="ml-auto shrink-0 text-legio-fg-subtle transition-transform motion-reduce:transition-none {open ? 'rotate-180' : ''}"
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
        <h3 class="text-sm font-semibold text-legio-fg">{title}</h3>
      </div>
    </div>
  {/if}
  {#if !collapsible || open}
    <div class="flex flex-col gap-4">
      {@render children()}
    </div>
  {/if}
</section>
