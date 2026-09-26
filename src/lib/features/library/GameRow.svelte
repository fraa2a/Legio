<script lang="ts">
  import type { Game } from "../../services/local-state";
  import type { GameLaunchState } from "../../services/steam-accounts";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";

  let {
    game,
    launch,
    cancelPending = false,
    actionPending = false,
    onPlay,
    onCancelLaunch,
    onStop,
    onDelete,
  }: {
    game: Game;
    launch: GameLaunchState | undefined;
    cancelPending?: boolean;
    actionPending?: boolean;
    onPlay: (game: Game) => void;
    onCancelLaunch: (game: Game) => void;
    onStop: (game: Game) => void;
    onDelete: (game: Game) => void;
  } = $props();

  const status = $derived(launch?.status ?? "idle");
  const isSteamGame = $derived(game.steamAppId !== null);
  const location = $derived(game.steamInstallPath ?? game.executablePath);
</script>

<li class="flex flex-wrap items-center gap-4 rounded-xl bg-white/5 p-4 light:bg-zinc-100">
  <div class="min-w-0 flex-1">
    <div class="flex flex-wrap items-center gap-2">
      <p class="truncate font-medium text-zinc-50 light:text-zinc-900">{game.name}</p>
      <Badge tone={isSteamGame ? "info" : "neutral"} title={isSteamGame ? "Steam" : "Manuale"} />
      {#if status !== "idle"}
        <Badge
          tone={status === "running" ? "success" : "warning"}
          title={status === "running" ? "In esecuzione" : "Avvio in corso"}
        />
      {/if}
    </div>
    {#if location}
      <p class="mt-1 truncate text-xs text-zinc-500">{location}</p>
    {/if}
    {#if launch?.error}
      <p class="mt-1 text-xs text-red-300 light:text-red-700" role="alert">{launch.error}</p>
    {/if}
  </div>

  <div class="flex items-center gap-2">
    {#if isSteamGame}
      {#if status === "idle"}
        <Button label="Gioca" variant="secondary" disabled={actionPending} onClick={() => onPlay(game)} />
      {:else if status === "launching"}
        <Button
          label={cancelPending ? "Annullamento..." : "Annulla avvio"}
          variant="secondary"
          disabled={cancelPending}
          onClick={() => onCancelLaunch(game)}
        />
      {:else}
        <Button
          label="Arresta"
          variant="secondary"
          disabled={actionPending}
          onClick={() => onStop(game)}
        />
      {/if}
    {/if}
    <Button label="Rimuovi" variant="danger" disabled={actionPending} onClick={() => onDelete(game)} />
  </div>
</li>
