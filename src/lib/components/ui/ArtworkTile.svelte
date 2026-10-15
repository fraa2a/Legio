<script lang="ts">
  import type { Snippet } from "svelte";
  import type { SteamAssetKind } from "../../services/steam-details";
  import { observeVisibility } from "../../utils/visibility";
  import ArtworkScope from "./ArtworkScope.svelte";
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

  const asset = $derived(assetOverride ?? (portrait ? "library_capsule" : "hero_blur"));
  const fallbackAsset = $derived(portrait || asset === "hero_blur" ? null : "header");

  let tile: HTMLElement | undefined = $state();
  let visible = $state(false);

  $effect(() => {
    return observeVisibility(tile, (value = true) => (visible = value), false);
  });
</script>

<ArtworkScope active={visible}>
  <li
    class="tile group relative flex min-h-max min-w-0 flex-col overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200 {portrait ? 'aspect-[2/3]' : 'aspect-[2.14/1]'} {className}"
  >
    <div
      bind:this={tile}
      class="legio-artwork-zoom absolute inset-0"
    >
      {#if visible && steamAppId !== null}
        <SteamArtwork
          {steamAppId}
          {asset}
          origin="visible"
          {fallbackAsset}
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
</ArtworkScope>

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
