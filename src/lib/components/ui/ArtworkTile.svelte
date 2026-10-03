<script lang="ts">
  import type { Snippet } from "svelte";
  import { peekSteamImage, type SteamAssetKind } from "../../services/steam-details";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import { observeVisibility } from "../../utils/visibility";
  import SteamArtwork from "../../features/library/SteamArtwork.svelte";

  let {
    steamAppId,
    monogram,
    portrait = false,
    asset: assetOverride = null,
    class: className = "",
    children,
  }: {
    steamAppId: number | null;
    monogram: string;
    portrait?: boolean;
    asset?: SteamAssetKind | null;
    class?: string;
    children: Snippet;
  } = $props();

  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const asset = $derived(assetOverride ?? (portrait ? "library_capsule" : "hero_blur"));
  const fallbackAsset = $derived(portrait ? null : "header");
  const cachedCover = $derived(steamAppId !== null && peekSteamImage({
    steamAppId,
    asset,
    fallbackAsset,
    index: null,
    version: detailsState?.cachedAt ?? null,
    full: false,
  }) !== undefined);

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
  class="tile group relative flex min-h-max min-w-0 flex-col overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200 {portrait ? 'aspect-[2/3]' : 'aspect-[2.14/1]'} {className}"
>
  <div
    bind:this={tile}
    class="absolute inset-0 transition-transform duration-300 ease-out group-hover:scale-105"
  >
    {#if (visible || cachedCover) && details !== null}
      <SteamArtwork
        steamAppId={details.steamAppId}
        {asset}
        {fallbackAsset}
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

<style>
  /* The hover zoom promotes the artwork to its own composited layer, and
     WebKitGTK on Hyprland does not apply the card's rounded overflow clip to it.
     The zoomed artwork then leaks into the corners with no overlay on top.
     An explicit clip-path is honoured on the promoted layer, so it restores the
     rounded shape; the card keeps overflow-hidden as the fallback elsewhere. */
  .tile {
    clip-path: inset(0 round 1rem);
  }
</style>
