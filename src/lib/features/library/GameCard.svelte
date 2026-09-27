<script lang="ts">
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import Badge from "../../components/ui/Badge.svelte";
  import GameLaunchControls from "./GameLaunchControls.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";

  let {
    game,
    launch,
    actionPending = false,
    cancelPending = false,
    onPlay,
    onCancel,
    onStop,
    onOpen,
  }: {
    game: Game;
    launch: GameLaunchState | undefined;
    actionPending?: boolean;
    cancelPending?: boolean;
    onPlay: (game: Game) => void;
    onCancel: (game: Game) => void;
    onStop: (game: Game) => void;
    onOpen: () => void;
  } = $props();

  const steamAppId = $derived(game.steamAppId);
  const status = $derived(launch?.status ?? "idle");
  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const monogram = $derived(game.name.trim().charAt(0).toUpperCase() || "?");

  let tile: HTMLElement | undefined = $state();
  let visible = $state(false);

  $effect(() => {
    if (steamAppId === null || visible) return;
    if (tile === undefined || typeof IntersectionObserver === "undefined") {
      visible = true;
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      visible = true;
      observer.disconnect();
    });
    observer.observe(tile);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (visible && steamAppId !== null) ensureSteamDetails(steamAppId);
  });
</script>

<li class="group relative flex aspect-[2.14/1] min-h-max flex-col overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200">
  <button
    type="button"
    bind:this={tile}
    class="flex flex-1 flex-col text-left"
    aria-label="Dettagli di {game.name}"
    onclick={onOpen}
  >
    <div class="absolute inset-0 transition-transform duration-300 ease-out group-hover:scale-105">
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

    {#if status !== "idle"}
      <div class="absolute right-2 top-2">
        <Badge tone={status === "running" ? "success" : "warning"} title={status === "running" ? "In esecuzione" : "Avvio in corso"} />
      </div>
    {/if}

    <div
      class="relative mt-auto flex flex-col gap-1 bg-gradient-to-t from-zinc-950/90 via-zinc-950/50 to-transparent px-4 pt-8 pb-3 light:from-zinc-100/90 light:via-zinc-100/50"
    >
      {#if launch?.error}
        <span class="text-xs text-red-300 light:text-red-700" role="alert">{launch.error}</span>
      {/if}
      <div class="flex items-center gap-2">
        <span class="min-w-0 flex-1 truncate font-medium text-zinc-50 light:text-zinc-900">{game.name}</span>
        <Badge tone={steamAppId === null ? "neutral" : "info"} title={steamAppId === null ? "Manuale" : "Steam"} />
        {#if game.nameOverride !== null}
          <Badge tone="warning" title="Nome personalizzato" />
        {/if}
      </div>
    </div>
  </button>

  {#if steamAppId !== null}
    <div class="pointer-events-none absolute inset-0 flex items-center justify-center [&_button]:pointer-events-auto">
      <GameLaunchControls
        {game}
        {launch}
        {actionPending}
        {cancelPending}
        circle={status === "idle"}
        variant="primary"
        revealOnHover
        {onPlay}
        {onCancel}
        {onStop}
      />
    </div>
  {/if}
</li>

{#snippet cover()}
  <div
    class="flex size-full items-center justify-center bg-gradient-to-br from-zinc-700/70 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  >
    <span class="text-4xl font-semibold text-zinc-400/80 light:text-zinc-500">{monogram}</span>
  </div>
{/snippet}
