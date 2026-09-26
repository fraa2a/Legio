<script lang="ts">
  import { inspectSteamGameLaunch } from "../../services/steam-accounts";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { deleteGame, games } from "../../stores/games";
  import {
    abortLaunch,
    hasPendingLaunch,
    launchStateByGame,
    launchStates,
    requestLaunch,
    stopRunningGame,
  } from "../../stores/launch";
  import GameRow from "./GameRow.svelte";

  let query = $state("");
  let actionError = $state<string | null>(null);
  let pendingGame = $state<string | null>(null);
  let cancelPending = $state<string | null>(null);
  let switchTarget = $state<Game | null>(null);
  let deleteTarget = $state<Game | null>(null);

  const filtered = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (needle.length === 0) return $games.data;
    return $games.data.filter((game) => game.name.toLowerCase().includes(needle));
  });

  $effect(() => {
    if (!$hasPendingLaunch) return;
    const timer = setInterval(() => void launchStates.load(), 2000);
    return () => clearInterval(timer);
  });

  async function play(game: Game): Promise<void> {
    actionError = null;
    pendingGame = game.id;
    try {
      const inspection = await inspectSteamGameLaunch(game.id);
      if (inspection.status === "mismatch") {
        switchTarget = game;
        return;
      }
      await requestLaunch(game.id, false);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pendingGame = null;
    }
  }

  async function confirmAccountSwitch(): Promise<void> {
    const game = switchTarget;
    switchTarget = null;
    if (game === null) return;
    actionError = null;
    pendingGame = game.id;
    try {
      await requestLaunch(game.id, true);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pendingGame = null;
    }
  }

  async function cancelLaunchFor(game: Game): Promise<void> {
    actionError = null;
    cancelPending = game.id;
    try {
      await abortLaunch(game.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      cancelPending = null;
    }
  }

  async function stop(game: Game): Promise<void> {
    actionError = null;
    pendingGame = game.id;
    try {
      await stopRunningGame(game.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pendingGame = null;
    }
  }

  async function confirmDelete(): Promise<void> {
    const game = deleteTarget;
    deleteTarget = null;
    if (game === null) return;
    actionError = null;
    pendingGame = game.id;
    try {
      await deleteGame(game.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pendingGame = null;
    }
  }
</script>

{#if actionError}
  <div class="mb-4">
    <ErrorBanner message={actionError} />
  </div>
{/if}

<div class="mb-6 flex flex-col gap-2">
  <label for="library-search" class="text-sm text-zinc-400 light:text-zinc-600">Cerca</label>
  <input
    id="library-search"
    type="search"
    bind:value={query}
    placeholder="Filtra per nome"
    class="w-full max-w-sm rounded-lg bg-white/5 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 light:bg-white light:text-zinc-900"
  />
</div>

<StateBlock
  status={$games.status}
  hasData={$games.data.length > 0}
  emptyMessage="Nessun gioco in libreria."
  error={$games.error}
  onRetry={() => void games.load()}
/>

{#if filtered.length > 0}
  <ul class="flex flex-col gap-3">
    {#each filtered as game (game.id)}
      <GameRow
        {game}
        launch={$launchStateByGame.get(game.id)}
        cancelPending={cancelPending === game.id}
        actionPending={pendingGame === game.id}
        onPlay={play}
        onCancelLaunch={cancelLaunchFor}
        onStop={stop}
        onDelete={(target) => (deleteTarget = target)}
      />
    {/each}
  </ul>
{:else if query.trim().length > 0}
  <p class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
    Nessun gioco corrisponde alla ricerca.
  </p>
{/if}

<Dialog open={switchTarget !== null} title="Cambio account Steam" onClose={() => (switchTarget = null)}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Steam deve essere chiuso e riavviato per usare l'account salvato di {switchTarget?.name}. Procedere?
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Annulla" variant="secondary" onClick={() => (switchTarget = null)} />
    <Button label="Riavvia e avvia" onClick={() => void confirmAccountSwitch()} />
  </div>
</Dialog>

<Dialog open={deleteTarget !== null} title="Rimuovi gioco" onClose={() => (deleteTarget = null)}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    {deleteTarget?.name} verrà rimosso dalla libreria. I file installati non vengono eliminati.
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Annulla" variant="secondary" onClick={() => (deleteTarget = null)} />
    <Button label="Rimuovi" variant="danger" onClick={() => void confirmDelete()} />
  </div>
</Dialog>
