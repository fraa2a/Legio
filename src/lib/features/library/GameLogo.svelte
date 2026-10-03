<script lang="ts">
  import type { Game } from "../../services/local-state";
  import { steamDetails } from "../../stores/steam-details";
  import SteamArtwork from "./SteamArtwork.svelte";

  let {
    game,
    class: className = "h-14 w-56 max-w-full object-contain object-center",
  }: {
    game: Game;
    class?: string;
  } = $props();

  const detailsState = $derived(game.steamAppId === null ? null : ($steamDetails[game.steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
</script>

{#if details !== null}
  <SteamArtwork
    steamAppId={details.steamAppId}
    asset="logo"
    version={detailsState?.cachedAt ?? null}
    caption={false}
    alt=""
    class={className}
  >
    {#snippet placeholder()}{@render namePill()}{/snippet}
  </SteamArtwork>
{:else}
  {@render namePill()}
{/if}

{#snippet namePill()}
  <span class="w-fit min-w-0 max-w-full truncate rounded-lg bg-zinc-950/60 px-3 py-1.5 font-medium text-zinc-50 light:bg-zinc-100/70 light:text-zinc-900">{game.name}</span>
{/snippet}
