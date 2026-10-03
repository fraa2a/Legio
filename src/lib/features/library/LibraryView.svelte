<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { games } from "../../stores/games";
  import { addGameDialogOpen, libraryPortrait, libraryQuery } from "../../stores/library-ui";
  import { launchError, launchStateByGame } from "../../stores/launch";
  import { openGame } from "../../stores/navigation";
  import { playtime } from "../../stores/playtime";
  import { importSteamLibrary, steamLibrary } from "../../stores/steam-library";
  import AddGameDialog from "./AddGameDialog.svelte";
  import GameCard from "./GameCard.svelte";

  const collator = new Intl.Collator(undefined, { sensitivity: "base" });
  onMount(() => {
    void playtime.load();
    const timer = setInterval(() => void playtime.load(), 30000);
    return () => clearInterval(timer);
  });

  const visible = $derived.by(() => {
    const needle = $libraryQuery.trim().toLowerCase();
    const groups: Record<string, typeof $games.data> = Object.create(null);
    for (const game of $games.data) {
      const key = game.steamAppId === null ? game.id : `steam:${game.steamAppId}`;
      const group = groups[key] ?? [];
      group.push(game);
      groups[key] = group;
    }
    return Object.values(groups)
      .filter((group) => needle.length === 0 || group.some((game) => game.name.toLowerCase().includes(needle)))
      .map((group) => ({
        game: group.find((game) => $launchStateByGame.get(game.id)?.status === "running" && (needle.length === 0 || game.name.toLowerCase().includes(needle)))
          ?? (needle.length > 0 ? group.find((game) => game.name.toLowerCase().includes(needle)) : undefined)
          ?? group.find((game) => game.steamInstallPath !== null)
          ?? group[0],
        totalMilliseconds: group.reduce((total, game) => total + ($playtime.data.find((summary) => summary.gameId === game.id)?.totalMilliseconds ?? 0), 0),
      }))
      .sort((left, right) => collator.compare(left.game.name, right.game.name));
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
    <ErrorBanner message={$steamLibrary.error} onRetry={() => void importSteamLibrary()} retryLabel={t("Riprova", $language)} />
  {/if}

  {#if $steamLibrary.importResult?.diagnostics.length}
    <ul class="rounded-xl border border-amber-400/20 bg-amber-400/5 px-4 py-3 text-xs text-amber-300 light:text-amber-800" aria-label={t("Diagnostica Steam", $language)}>
      {#each $steamLibrary.importResult.diagnostics as diagnostic (diagnostic)}<li>{diagnostic}</li>{/each}
    </ul>
  {/if}

  <StateBlock
    status={listStatus}
    hasData={$games.data.length > 0}
    emptyMessage={t("Nessun gioco in libreria. Steam viene sincronizzato automaticamente, oppure puoi aggiungere un gioco con +.", $language)}
    error={$games.error}
    onRetry={() => void games.load()}
  />

  {#if visible.length > 0}
    <ul class="grid auto-rows-max gap-4 {$libraryPortrait ? 'grid-cols-[repeat(auto-fill,minmax(min(100%,11rem),1fr))]' : 'grid-cols-[repeat(auto-fill,minmax(min(100%,18rem),1fr))]'}">
      {#each visible as item (item.game.id)}
        <GameCard
          game={item.game}
          portrait={$libraryPortrait}
          playtimeMilliseconds={item.totalMilliseconds}
          launch={$launchStateByGame.get(item.game.id)}
          onOpen={() => openGame(item.game.id)}
        />
      {/each}
    </ul>
  {:else if $games.data.length > 0}
    <div class="rounded-xl border border-white/10 bg-white/[0.03] p-6 text-zinc-400 light:border-zinc-900/10 light:bg-zinc-100 light:text-zinc-600">
      <p>{t("Nessun gioco corrisponde alla ricerca.", $language)}</p>
    </div>
  {/if}

</div>

{#if $addGameDialogOpen}
  <AddGameDialog onClose={() => addGameDialogOpen.set(false)} />
{/if}
