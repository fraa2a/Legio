<script lang="ts">
  import type { Game } from "../../services/local-state";
  import SteamArtwork from "../../features/library/SteamArtwork.svelte";
  import { openGame } from "../../stores/navigation";
  import { observeVisibility } from "../../utils/visibility";

  let {
    game,
    selected = false,
    expanded = true,
  }: {
    game: Game;
    selected?: boolean;
    expanded?: boolean;
  } = $props();

  let item: HTMLElement | undefined = $state();
  let visible = $state(false);

  $effect(() => {
    if (visible) return;
    return observeVisibility(item, () => (visible = true));
  });
</script>

<li>
  <button
    type="button"
    aria-label={`Apri ${game.name}`}
    aria-current={selected ? "true" : undefined}
    class="group relative flex h-12 w-full items-center overflow-hidden rounded-lg border border-white/10 transition-[border-color,background-color,box-shadow] duration-300 ease-out focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-white/80 light:border-zinc-900/15 light:focus-visible:ring-zinc-900/60 {expanded
      ? 'px-2.5 text-left hover:border-white/20 hover:shadow-md hover:shadow-black/30 light:hover:border-zinc-900/30'
      : ''} {selected
      ? 'border-white/30 light:border-zinc-900/40'
      : ''}"
    onclick={() => openGame(game.id)}
  >
    <span bind:this={item} class="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
      <span class="absolute -inset-2 transition-transform duration-200 ease-out group-hover:scale-105">
        {#if visible && game.steamAppId !== null}
          <SteamArtwork
            steamAppId={game.steamAppId}
            asset="header"
            caption={false}
            class="size-full object-cover"
          >
            {#snippet placeholder()}{@render backgroundFallback()}{/snippet}
          </SteamArtwork>
        {:else}
          {@render backgroundFallback()}
        {/if}
      </span>
      <span class="absolute inset-0 bg-zinc-950/60 transition-colors duration-200 group-hover:bg-zinc-950/45 light:bg-zinc-100/65 light:group-hover:bg-zinc-100/50"></span>
    </span>

    <span
      class="relative min-w-0 flex-1 truncate text-sm font-medium text-white transition-opacity duration-300 ease-out light:text-zinc-900 {expanded
        ? 'opacity-100'
        : 'opacity-0'}"
    >
      {game.name}
    </span>
  </button>
</li>

{#snippet backgroundFallback()}
  <span class="block size-full bg-gradient-to-br from-zinc-600/80 to-zinc-900 light:from-zinc-300 light:to-zinc-100"></span>
{/snippet}