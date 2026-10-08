<script lang="ts">
  import type { Game } from "../../services/local-state";
  import { acquireGameArtwork, gameArtworkRevision } from "../../services/game-artwork";
  import SteamArtwork from "./SteamArtwork.svelte";

  let { game, class: className = "size-8 rounded object-contain" }: { game: Game; class?: string } = $props();
  let url = $state<string | null>(null);
  let loading = $state(true);
  const gameId = $derived(game.id);

  $effect(() => {
    const id = gameId;
    void $gameArtworkRevision[`${id}:icon`];
    const artwork = acquireGameArtwork(id, "icon");
    let cancelled = false;
    url = artwork.url;
    loading = url === null;
    void artwork.ready.then((value) => { if (!cancelled) url = value; }).catch((error: unknown) => {
      console.warn("Could not load local game icon", error);
    }).finally(() => { if (!cancelled) loading = false; });
    return () => { cancelled = true; artwork.release(); };
  });
</script>

{#if url !== null}
  <img src={url} alt="" draggable="false" class={className} onerror={() => { console.warn("Could not display local game icon", gameId); url = null; }} />
{:else if !loading && game.steamAppId !== null}
  <SteamArtwork steamAppId={game.steamAppId} asset="client_icon" caption={false} alt="" class={className}>
    {#snippet placeholder()}{@render monogram()}{/snippet}
  </SteamArtwork>
{:else}
  {@render monogram()}
{/if}

{#snippet monogram()}
  <span class="flex items-center justify-center bg-zinc-700/60 text-sm font-semibold text-zinc-100 light:bg-zinc-200 light:text-zinc-800 {className}" aria-hidden="true">{game.name.trim().charAt(0).toUpperCase() || "?"}</span>
{/snippet}
