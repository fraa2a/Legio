<script lang="ts">
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { games, manualGameCount, steamGameCount } from "../../stores/games";
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
  import { scanSteamLibrary } from "../../stores/steam-library";
  import AddGameDialog from "./AddGameDialog.svelte";
  import GameCard from "./GameCard.svelte";
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
  let addGameOpen = $state(false);
  let steamScanOpen = $state(false);

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

  function resetFilters(): void {
    query = "";
    sourceFilter = "all";
    sortOrder = "name-asc";
  }

  function openSteamScan(): void {
    steamScanOpen = true;
    void scanSteamLibrary();
  }
</script>

<div class="flex min-h-full flex-col gap-4">
  <div class="flex flex-wrap items-end justify-between gap-4">
    <div class="flex flex-wrap items-end gap-4">
      <div class="w-64">
        <TextField id="library-search" label="Cerca" type="search" bind:value={query} placeholder="Filtra per nome" />
      </div>
      <SelectField
        id="library-source"
        label="Origine"
        class="w-48"
        bind:value={sourceFilter}
        options={sourceOptions}
      />
      <SelectField
        id="library-sort"
        label="Ordinamento"
        class="w-48"
        bind:value={sortOrder}
        options={sortOptions}
      />
    </div>
    <div class="flex flex-wrap self-start gap-2">
      {#if hasFilters}
        <Button label="Azzera filtri" variant="secondary" onClick={resetFilters} />
      {/if}
      <Button label="Rileva Steam" variant="secondary" onClick={openSteamScan} />
      <Button label="Aggiungi gioco" onClick={() => (addGameOpen = true)} />
    </div>
  </div>

  {#if $launchError}
    <ErrorBanner message={$launchError} />
  {/if}

  <StateBlock
    status={$games.status}
    hasData={$games.data.length > 0}
    emptyMessage="Nessun gioco in libreria. Rileva le installazioni Steam o aggiungi un gioco manualmente."
    error={$games.error}
    onRetry={() => void games.load()}
  />

  {#if visible.length > 0}
    <ul class="grid gap-4 sm:grid-cols-2 2xl:grid-cols-3">
      {#each visible as game (game.id)}
        <GameCard
          {game}
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
    <div class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
      <p>Nessun gioco corrisponde a ricerca e filtri.</p>
      <div class="mt-4">
        <Button label="Azzera filtri" variant="secondary" onClick={resetFilters} />
      </div>
    </div>
  {/if}

  <p class="mt-auto pt-4 text-sm text-zinc-400 light:text-zinc-600">
    {$games.data.length} giochi · {$steamGameCount} da Steam · {$manualGameCount} manuali
  </p>
</div>

{#if addGameOpen}
  <AddGameDialog onClose={() => (addGameOpen = false)} />
{/if}

{#if steamScanOpen}
  <SteamScanDialog onClose={() => (steamScanOpen = false)} />
{/if}
