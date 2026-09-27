<script lang="ts">
  import { onMount } from "svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { games } from "../../stores/games";
  import { addGameDialogOpen, libraryQuery } from "../../stores/library-ui";
  import {
    abortGameLaunch,
    cancelPendingGameId,
    launchError,
    launchStateByGame,
    pendingGameId,
    playGame,
    stopGameProcess,
  } from "../../stores/launch";
  import { openGame } from "../../stores/navigation";
  import { playtime } from "../../stores/playtime";
  import { importSteamLibrary, steamLibrary } from "../../stores/steam-library";
  import AddGameDialog from "./AddGameDialog.svelte";
  import GameCard from "./GameCard.svelte";

  const collator = new Intl.Collator("it", { sensitivity: "base" });
  onMount(() => {
    void playtime.load();
    const timer = setInterval(() => void playtime.load(), 30000);
    return () => clearInterval(timer);
  });

  const visible = $derived.by(() => {
    const needle = $libraryQuery.trim().toLowerCase();
    return $games.data
      .filter((game) => needle.length === 0 || game.name.toLowerCase().includes(needle))
      .sort((left, right) => collator.compare(left.name, right.name));
  });
  const listStatus = $derived(
    $steamLibrary.importing && $games.data.length === 0 ? "loading" : $games.status,
  );
</script>

<div class="flex min-h-full flex-col gap-4">
  {#if $launchError}
    <ErrorBanner message={$launchError} />
  {/if}

  {#if $steamLibrary.error !== null}
    <ErrorBanner message={$steamLibrary.error} onRetry={() => void importSteamLibrary()} retryLabel="Riprova" />
  {/if}

  {#if $steamLibrary.importResult?.diagnostics.length}
    <ul class="rounded-xl border border-amber-400/20 bg-amber-400/5 px-4 py-3 text-xs text-amber-300 light:text-amber-800" aria-label="Diagnostica Steam">
      {#each $steamLibrary.importResult.diagnostics as diagnostic (diagnostic)}<li>{diagnostic}</li>{/each}
    </ul>
  {/if}

  <StateBlock
    status={listStatus}
    hasData={$games.data.length > 0}
    emptyMessage="Nessun gioco in libreria. Steam viene sincronizzato automaticamente, oppure puoi aggiungere un gioco con +."
    error={$games.error}
    onRetry={() => void games.load()}
  />

  {#if visible.length > 0}
    <ul class="grid gap-4 sm:grid-cols-2 2xl:grid-cols-3">
      {#each visible as game (game.id)}
        <GameCard
          {game}
          playtimeMilliseconds={$playtime.data.find((summary) => summary.gameId === game.id)?.totalMilliseconds ?? 0}
          launch={$launchStateByGame.get(game.id)}
          cancelPending={$cancelPendingGameId === game.id}
          actionPending={$pendingGameId === game.id}
          onPlay={playGame}
          onCancel={abortGameLaunch}
          onStop={stopGameProcess}
          onOpen={() => openGame(game.id)}
        />
      {/each}
    </ul>
  {:else if $games.data.length > 0}
    <div class="rounded-xl border border-white/10 bg-white/[0.03] p-6 text-zinc-400 light:border-zinc-900/10 light:bg-zinc-100 light:text-zinc-600">
      <p>Nessun gioco corrisponde alla ricerca.</p>
    </div>
  {/if}

</div>

{#if $addGameDialogOpen}
  <AddGameDialog onClose={() => addGameDialogOpen.set(false)} />
{/if}
