<script lang="ts">
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import ArtworkTile from "../../components/ui/ArtworkTile.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import GameLaunchControls from "./GameLaunchControls.svelte";

  let {
    game,
    playtimeMilliseconds = 0,
    launch,
    actionPending = false,
    cancelPending = false,
    onPlay,
    onCancel,
    onStop,
    onOpen,
  }: {
    game: Game;
    playtimeMilliseconds?: number;
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
  const monogram = $derived(game.name.trim().charAt(0).toUpperCase() || "?");
</script>

<ArtworkTile {steamAppId} {monogram}>
  <button
    type="button"
    class="flex flex-1 flex-col text-left focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-white"
    aria-label="Dettagli di {game.name}"
    onclick={onOpen}
  >
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
      <div class="flex items-center gap-2 pr-28">
        <span class="min-w-0 flex-1 truncate font-medium text-zinc-50 light:text-zinc-900">{game.name}</span>
      </div>
      <span class="text-xs text-zinc-300 light:text-zinc-700">
        {#if playtimeMilliseconds >= 3600000}
          {Math.floor(playtimeMilliseconds / 3600000)} h {Math.floor((playtimeMilliseconds % 3600000) / 60000)} min
        {:else}
          {Math.floor(playtimeMilliseconds / 60000)} min
        {/if} giocati
      </span>
    </div>
  </button>

  {#if game.steamInstallPath !== null || game.executablePath !== null}
    <div class="absolute bottom-3 right-3 z-10">
      <GameLaunchControls
        {game}
        {launch}
        {actionPending}
        {cancelPending}
        variant="primary"
        class="h-12 min-w-24 shadow-lg shadow-black/30"
        {onPlay}
        {onCancel}
        {onStop}
      />
    </div>
  {/if}
</ArtworkTile>
