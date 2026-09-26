<script lang="ts">
  import { inspectSteamGameLaunch } from "../../services/steam-accounts";
  import type { Game } from "../../services/local-state";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { deleteGame, games, manualGameCount, steamGameCount } from "../../stores/games";
  import { scanSteamLibrary } from "../../stores/steam-library";
  import {
    abortLaunch,
    hasPendingLaunch,
    launchStateByGame,
    launchStates,
    requestLaunch,
    stopRunningGame,
  } from "../../stores/launch";
  import AddGameDialog from "./AddGameDialog.svelte";
  import GameDetailsDialog from "./GameDetailsDialog.svelte";
  import GameRow from "./GameRow.svelte";
  import SteamScanDialog from "./SteamScanDialog.svelte";

  type SourceFilter = "all" | "steam" | "manual";
  type SortOrder = "name-asc" | "name-desc" | "steam-first" | "manual-first";

  const sourceOptions: { value: string; label: string }[] = [
    { value: "all", label: "Tutti" },
    { value: "steam", label: "Steam" },
    { value: "manual", label: "Manuali" },
  ];

  const sortOptions: { value: string; label: string }[] = [
    { value: "name-asc", label: "Nome (A-Z)" },
    { value: "name-desc", label: "Nome (Z-A)" },
    { value: "steam-first", label: "Prima i giochi Steam" },
    { value: "manual-first", label: "Prima i giochi manuali" },
  ];

  const collator = new Intl.Collator("it", { sensitivity: "base" });

  let query = $state("");
  let sourceFilter = $state<SourceFilter>("all");
  let sortOrder = $state<SortOrder>("name-asc");
  let actionError = $state<string | null>(null);
  let pendingGame = $state<string | null>(null);
  let cancelPending = $state<string | null>(null);
  let switchTarget = $state<Game | null>(null);
  let deleteTarget = $state<Game | null>(null);
  let detailsId = $state<string | null>(null);
  let addGameOpen = $state(false);
  let steamScanOpen = $state(false);

  const detailsGame = $derived($games.data.find((game) => game.id === detailsId) ?? null);
  const hasFilters = $derived(
    query.trim().length > 0 || sourceFilter !== "all" || sortOrder !== "name-asc",
  );

  const visible = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    const filtered = $games.data.filter((game) => {
      if (needle.length > 0 && !game.name.toLowerCase().includes(needle)) return false;
      if (sourceFilter === "steam") return game.steamAppId !== null;
      if (sourceFilter === "manual") return game.steamAppId === null;
      return true;
    });
    return [...filtered].sort((left, right) => {
      if (sortOrder === "steam-first" || sortOrder === "manual-first") {
        const leftSteam = left.steamAppId !== null;
        const rightSteam = right.steamAppId !== null;
        if (leftSteam !== rightSteam) {
          return sortOrder === "steam-first"
            ? Number(rightSteam) - Number(leftSteam)
            : Number(leftSteam) - Number(rightSteam);
        }
      }
      const byName = collator.compare(left.name, right.name);
      return sortOrder === "name-desc" ? -byName : byName;
    });
  });

  $effect(() => {
    if (!$hasPendingLaunch) return;
    const timer = setInterval(() => void launchStates.load(), 2000);
    return () => clearInterval(timer);
  });

  function resetFilters(): void {
    query = "";
    sourceFilter = "all";
    sortOrder = "name-asc";
  }

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

  function requestDelete(game: Game): void {
    detailsId = null;
    deleteTarget = game;
  }

  function openSteamScan(): void {
    actionError = null;
    steamScanOpen = true;
    void scanSteamLibrary();
  }
</script>

<div class="mb-6 flex flex-wrap items-center justify-between gap-3">
  <p class="text-sm text-zinc-400 light:text-zinc-600">
    {$games.data.length} giochi · {$steamGameCount} da Steam · {$manualGameCount} manuali
  </p>
  <div class="flex flex-wrap gap-2">
    <Button label="Rileva Steam" variant="secondary" onClick={openSteamScan} />
    <Button label="Aggiungi gioco" onClick={() => (addGameOpen = true)} />
  </div>
</div>

{#if actionError}
  <div class="mb-4">
    <ErrorBanner message={actionError} />
  </div>
{/if}

<div class="mb-6 flex flex-wrap items-end gap-4">
  <div class="w-full max-w-sm">
    <TextField id="library-search" label="Cerca" type="search" bind:value={query} placeholder="Filtra per nome" />
  </div>
  <SelectField id="library-source" label="Origine" bind:value={sourceFilter} options={sourceOptions} />
  <SelectField id="library-sort" label="Ordinamento" bind:value={sortOrder} options={sortOptions} />
  {#if hasFilters}
    <Button label="Azzera filtri" variant="secondary" onClick={resetFilters} />
  {/if}
</div>

<StateBlock
  status={$games.status}
  hasData={$games.data.length > 0}
  emptyMessage="Nessun gioco in libreria. Rileva le installazioni Steam o aggiungi un gioco manualmente."
  error={$games.error}
  onRetry={() => void games.load()}
/>

{#if visible.length > 0}
  <ul class="flex flex-col gap-3">
    {#each visible as game (game.id)}
      <GameRow
        {game}
        launch={$launchStateByGame.get(game.id)}
        cancelPending={cancelPending === game.id}
        actionPending={pendingGame === game.id}
        onPlay={play}
        onCancelLaunch={cancelLaunchFor}
        onStop={stop}
        onSettings={(target) => (detailsId = target.id)}
        onDelete={(target) => (deleteTarget = target)}
      />
    {/each}
  </ul>
{:else if $games.data.length > 0}
  <div class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
    <p>Nessun gioco corrisponde a ricerca e filtri.</p>
    <div class="mt-4">
      <Button label="Azzera filtri" variant="secondary" onClick={resetFilters} />
    </div>
  </div>
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

{#if addGameOpen}
  <AddGameDialog onClose={() => (addGameOpen = false)} />
{/if}

{#if steamScanOpen}
  <SteamScanDialog onClose={() => (steamScanOpen = false)} />
{/if}

{#if detailsGame !== null}
  {#key detailsGame.id}
    <GameDetailsDialog
      game={detailsGame}
      onClose={() => (detailsId = null)}
      onDelete={requestDelete}
    />
  {/key}
{/if}
