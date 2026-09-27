<script lang="ts">
  import type { Snippet } from "svelte";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import { observeVisibility } from "../../utils/visibility";
  import SteamArtwork from "../../features/library/SteamArtwork.svelte";

  let {
    steamAppId,
    monogram,
    children,
  }: {
    steamAppId: number | null;
    monogram: string;
    children: Snippet;
  } = $props();

  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);

  let tile: HTMLElement | undefined = $state();
  let visible = $state(false);

  $effect(() => {
    if (visible) return;
    return observeVisibility(tile, () => (visible = true));
  });

  $effect(() => {
    if (visible && steamAppId !== null) ensureSteamDetails(steamAppId);
  });
</script>

<li
  class="group relative flex aspect-[2.14/1] min-h-max flex-col overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200"
>
  <div
    bind:this={tile}
    class="absolute inset-0 transition-transform duration-300 ease-out group-hover:scale-105"
  >
    {#if visible && details !== null}
      <SteamArtwork
        steamAppId={details.steamAppId}
        asset="header"
        version={detailsState?.cachedAt ?? null}
        caption={false}
        class="size-full object-cover"
      >
        {#snippet placeholder()}{@render cover()}{/snippet}
      </SteamArtwork>
    {:else}
      {@render cover()}
    {/if}
  </div>

  <div
    class="pointer-events-none absolute inset-0 bg-gradient-to-t from-zinc-950 via-zinc-950/40 to-zinc-950/20 transition-opacity duration-300 group-hover:opacity-0 light:from-zinc-100/80 light:via-transparent light:to-transparent"
    aria-hidden="true"
  ></div>

  <div class="relative flex min-h-0 flex-1 flex-col">
    {@render children()}
  </div>
</li>

{#snippet cover()}
  <div
    class="flex size-full items-center justify-center bg-gradient-to-br from-zinc-700/70 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  >
    <span class="text-4xl font-semibold text-zinc-400/80 light:text-zinc-500">{monogram}</span>
  </div>
{/snippet}
