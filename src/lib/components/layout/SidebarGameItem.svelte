<script lang="ts">
  import type { Game } from "../../services/local-state";
  import SteamArtwork from "../../features/library/SteamArtwork.svelte";
  import { openGame } from "../../stores/navigation";
  import { fade } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { observeVisibility } from "../../utils/visibility";
  import { fadeDuration } from "../../utils/motion";

  let {
    game,
    selected = false,
    expanded = true,
  }: {
    game: Game;
    selected?: boolean;
    expanded?: boolean;
  } = $props();

  let icon: HTMLElement | undefined = $state();
  let visible = $state(false);

  $effect(() => {
    if (visible) return;
    return observeVisibility(icon, () => (visible = true));
  });
</script>

<li>
  <button
    type="button"
    aria-label={`Apri ${game.name}`}
    aria-current={selected ? "true" : undefined}
    class="group relative flex h-12 w-full items-center overflow-hidden rounded-lg border border-transparent transition-[border-color,background-color,box-shadow] duration-300 ease-out hover:shadow-md hover:shadow-black/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-white/80 light:focus-visible:ring-zinc-900/60 {selected
      ? 'border-white/30 light:border-zinc-900/40'
      : ''}"
    onclick={() => openGame(game.id)}
  >
    <span
      bind:this={icon}
      class="pointer-events-none absolute top-1/2 left-2 flex size-8 -translate-y-1/2 items-center justify-center"
      aria-hidden="true"
    >
      {#if visible && game.steamAppId !== null}
        <SteamArtwork
          steamAppId={game.steamAppId}
          asset="client_icon"
          caption={false}
          class="size-8 rounded object-cover"
        >
          {#snippet placeholder()}{/snippet}
        </SteamArtwork>
      {/if}
    </span>
    {#if expanded}
      <span
        transition:fade={{ duration: fadeDuration, easing: cubicOut }}
        class="absolute top-1/2 left-12 w-36 -translate-y-1/2 truncate text-left text-sm font-medium text-white light:text-zinc-900"
      >
        {game.name}
      </span>
    {/if}
  </button>
</li>
