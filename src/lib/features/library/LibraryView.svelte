<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import { startVisibleRefresh } from "../../stores/visible-refresh";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import type { LibrarySortKey, LibraryViewPreferences } from "../../services/local-state";
  import { games } from "../../stores/games";
  import { addGameDialogOpen, libraryPortrait, libraryQuery, libraryView, previewLibraryView, updateLibraryView } from "../../stores/library-ui";
  import { libraryOrganization, organizationByGame, setFavorite } from "../../stores/library-organization";
  import { launchStateByGame } from "../../stores/launch";
  import { openGame } from "../../stores/navigation";
  import { playtime } from "../../stores/playtime";
  import { importSteamLibrary, steamLibrary } from "../../stores/steam-library";
  import { settings } from "../../stores/settings";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import AddGameDialog from "./AddGameDialog.svelte";
  import CollectionsDialog from "./CollectionsDialog.svelte";
  import GameCard from "./GameCard.svelte";
  import LibraryList from "./LibraryList.svelte";
  import LibraryToolbar from "./LibraryToolbar.svelte";
  import OrganizeGameDialog from "./OrganizeGameDialog.svelte";
  import { reflowOnResize } from "./grid-reflow";
  import { libraryGroups, libraryItems } from "./library-model";
  import { collectionMembership, filterEntries, libraryEntries, libraryShelves, nextSort, sortEntries, type LibraryEntry } from "./library-organize";

  onMount(() => startVisibleRefresh(() => void playtime.load(), 30000));

  // Membership and insertion times follow games that were added or removed.
  $effect(() => {
    void $games.data;
    void libraryOrganization.load();
  });

  let organizingId = $state<string | null>(null);
  let collectionsOpen = $state(false);

  const groups = $derived(libraryGroups($games.data, $libraryQuery));
  const summaries = $derived(new Map($playtime.data.map((summary) => [summary.gameId, summary])));
  const collections = $derived($libraryOrganization.data.collections);
  const entries = $derived(libraryEntries(
    libraryItems(groups, $libraryQuery, summaries, $launchStateByGame),
    summaries,
    $organizationByGame,
    collectionMembership(collections),
  ));
  const visible = $derived(sortEntries(filterEntries(entries, $libraryView, collections), $libraryView.sort, $libraryView.descending));
  const shelves = $derived($libraryView.mode === "shelves" ? libraryShelves(visible, collections, t("Senza raccolta", $language)) : []);
  const organizing = $derived(organizingId === null ? null : (entries.find((entry) => entry.game.id === organizingId) ?? null));
  const tileWidth = $derived(`${($libraryPortrait ? 11 : 18) * $libraryView.coverScale / 100}rem`);
  const listStatus = $derived(
    $steamLibrary.importing && $games.data.length === 0 ? "loading" : $games.status,
  );

  function reportError(error: unknown): void {
    showToast(toMessage(error), "error");
  }

  function changeView(changes: Partial<LibraryViewPreferences>): void {
    updateLibraryView(changes).catch(reportError);
  }

  function sortBy(key: LibrarySortKey): void {
    changeView(nextSort($libraryView, key));
  }

  function toggleFavorite(entry: LibraryEntry): void {
    setFavorite(entry.gameIds, !entry.favorite).catch(reportError);
  }
</script>

<div class="flex min-h-full flex-col gap-4">

  {#if $settings.data.syncSteamGames && $steamLibrary.error !== null}
    <ErrorBanner message={$steamLibrary.error} onRetry={() => void importSteamLibrary()} retryLabel={t("Riprova", $language)} />
  {/if}

  {#if $steamLibrary.importResult?.diagnostics.length}
    <Callout tone="warning" title={t("Diagnostica Steam", $language)} items={$steamLibrary.importResult.diagnostics} />
  {/if}

  {#if $libraryOrganization.error !== null}
    <ErrorBanner message={$libraryOrganization.error} onRetry={() => void libraryOrganization.reload()} retryLabel={t("Riprova", $language)} />
  {/if}

  <StateBlock
    status={listStatus}
    hasData={$games.data.length > 0}
    skeletonRows={3}
    skeletonClass="h-32"
    emptyIcon="library"
    emptyMessage={$settings.data.syncSteamGames
      ? t("Nessun gioco visibile in libreria. Puoi aggiungerne uno con + o riabilitare i giochi Steam nascosti nelle impostazioni delle librerie.", $language)
      : t("Nessun gioco visibile in libreria. Puoi aggiungerne uno con + o riabilitare i giochi Steam nascosti nelle impostazioni delle librerie. Sync Steam games è disattivato.", $language)}
    error={$games.error}
    onRetry={() => void games.load()}
  />

  {#if $games.data.length > 0}
    <LibraryToolbar
      view={$libraryView}
      {collections}
      onChange={changeView}
      onPreviewCoverScale={(coverScale) => previewLibraryView({ coverScale })}
      onCommitCoverScale={() => changeView({})}
      onManageCollections={() => (collectionsOpen = true)}
    />
  {/if}

  {#snippet card(entry: LibraryEntry, className = "")}
    <GameCard
      game={entry.game}
      portrait={$libraryPortrait}
      playtimeMilliseconds={entry.totalMilliseconds}
      launch={$launchStateByGame.get(entry.game.id)}
      favorite={entry.favorite}
      onToggleFavorite={() => toggleFavorite(entry)}
      onOrganize={() => (organizingId = entry.game.id)}
      onOpen={() => openGame(entry.game.id)}
      class={className}
    />
  {/snippet}

  {#if visible.length > 0}
    {#if $libraryView.mode === "list"}
      <LibraryList
        entries={visible}
        sort={$libraryView.sort}
        descending={$libraryView.descending}
        onSort={sortBy}
        onOpen={(entry) => openGame(entry.game.id)}
        onToggleFavorite={toggleFavorite}
        onOrganize={(entry) => (organizingId = entry.game.id)}
      />
    {:else if $libraryView.mode === "shelves"}
      <div class="flex flex-col gap-6" style:--library-tile-width={tileWidth}>
        {#each shelves as shelf (shelf.id ?? "")}
          <section class="flex min-w-0 flex-col gap-2" aria-label={shelf.name}>
            <h2 class="text-base font-semibold text-legio-fg">
              {shelf.name} <span class="text-sm font-normal text-legio-fg-muted tabular-nums">{shelf.entries.length}</span>
            </h2>
            <ul class="flex gap-4 overflow-x-auto pb-2">
              {#each shelf.entries as entry (entry.game.id)}
                {@render card(entry, "w-(--library-tile-width) shrink-0")}
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {:else}
      <ul
        use:reflowOnResize={libraryPortrait}
        style:--library-tile-width={tileWidth}
        class="relative grid auto-rows-max grid-cols-[repeat(auto-fill,minmax(min(100%,var(--library-tile-width)),1fr))] gap-4"
      >
        {#each visible as entry (entry.game.id)}
          {@render card(entry)}
        {/each}
      </ul>
    {/if}
  {:else if $games.data.length > 0}
    <div class="rounded-card legio-glass p-6 text-legio-fg-muted">
      <p>{$libraryQuery.trim() ? t("Nessun gioco corrisponde alla ricerca.", $language) : t("Nessun gioco corrisponde ai filtri.", $language)}</p>
    </div>
  {/if}

</div>

{#if $addGameDialogOpen}
  <AddGameDialog onClose={() => addGameDialogOpen.set(false)} />
{/if}

{#if organizing !== null}
  <OrganizeGameDialog
    name={organizing.game.name}
    gameIds={organizing.gameIds}
    playStatus={organizing.playStatus}
    onClose={() => (organizingId = null)}
  />
{/if}

{#if collectionsOpen}
  <CollectionsDialog onClose={() => (collectionsOpen = false)} />
{/if}
