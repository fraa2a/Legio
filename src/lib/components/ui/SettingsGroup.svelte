<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon, { type IconName } from "./Icon.svelte";

  let {
    title,
    icon,
    children,
  }: {
    title: string;
    icon?: IconName;
    children: Snippet;
  } = $props();
</script>

<section
  class="flex flex-col gap-4 rounded-2xl bg-white/5 p-5 shadow-sm shadow-black/20 light:bg-zinc-100 light:shadow-zinc-900/5"
>
  <div class="flex items-center gap-3">
    {#if icon}
      <span
        class="grid size-8 shrink-0 place-items-center rounded-lg bg-white/10 text-zinc-200 light:bg-zinc-900/10 light:text-zinc-800"
      >
        <Icon name={icon} size="size-4" />
      </span>
    {/if}
    <div class="min-w-0">
      <h3 class="text-sm font-semibold text-zinc-100 light:text-zinc-900">{title}</h3>
    </div>
  </div>
  <div class="flex flex-col gap-4">
    {@render children()}
  </div>
</section>
