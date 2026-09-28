<script lang="ts">
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import ArtworkTile from "../../components/ui/ArtworkTile.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import Icon from "../../components/ui/Icon.svelte";

  let {
    game,
    playtimeMilliseconds = 0,
    launch,
    onOpen,
  }: {
    game: Game;
    playtimeMilliseconds?: number;
    launch: GameLaunchState | undefined;
    onOpen: () => void;
  } = $props();

  const steamAppId = $derived(game.steamAppId);
  const status = $derived(launch?.status ?? "idle");
  const monogram = $derived(game.name.trim().charAt(0).toUpperCase() || "?");

  const playtimeLabel = $derived.by(() => {
    const hours = Math.floor(playtimeMilliseconds / 3600000);
    if (hours >= 1) return hours === 1 ? "1h" : `${hours}h`;
    return `${Math.floor(playtimeMilliseconds / 60000)}m`;
  });
</script>

<ArtworkTile {steamAppId} {monogram}>
  <button
    type="button"
    class="flex flex-1 flex-col text-left focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-white"
    aria-label="Dettagli di {game.name}"
    onclick={onOpen}
  >
    <div class="absolute right-2 top-2 flex items-center gap-1.5">
      {#if status !== "idle"}
        <Badge tone={status === "running" ? "success" : "warning"} title={status === "running" ? "In esecuzione" : "Avvio in corso"} />
      {/if}
      <span
        class="flex items-center gap-1 rounded-full bg-zinc-950/60 px-2 py-0.5 text-xs font-medium tabular-nums text-zinc-100 light:bg-zinc-100/70 light:text-zinc-900"
        title="{playtimeLabel} giocati"
      >
        <Icon name="clock" size="h-3.5 w-3.5" />
        {playtimeLabel}
      </span>
    </div>

    <div class="relative mt-auto p-3">
      <div
        class="flex w-fit min-w-0 max-w-full flex-col gap-0.5 rounded-lg bg-zinc-950/60 px-3 py-1.5 light:bg-zinc-100/70"
      >
        {#if launch?.error}
          <span class="text-xs text-red-300 light:text-red-700" role="alert">{launch.error}</span>
        {/if}
        <span class="truncate font-medium text-zinc-50 light:text-zinc-900">{game.name}</span>
      </div>
    </div>
  </button>
</ArtworkTile>
