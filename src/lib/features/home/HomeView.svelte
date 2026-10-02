<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { games } from "../../stores/games";
  import { accountSwitchGame, confirmAccountSwitch, dismissAccountSwitch, launchError, launchStateByGame, launchStates } from "../../stores/launch";
  import { openGame, selectSection } from "../../stores/navigation";
  import { playtime } from "../../stores/playtime";
  import GameCard from "../library/GameCard.svelte";
  import GameSettingsDialog from "../library/GameSettingsDialog.svelte";
  import ActivityCalendar from "./ActivityCalendar.svelte";
  import LastPlayedHero from "./LastPlayedHero.svelte";
  import { recentGames } from "./home-model";

  const recent = $derived(recentGames($games.data, $playtime.data));
  const lastPlayed = $derived(recent[0] ?? null);
  let settingsGameId = $state<string | null>(null);
  const settingsGame = $derived($games.data.find((game) => game.id === settingsGameId) ?? null);
  const loading = $derived(($games.status === 'idle' || $games.status === 'loading' || $playtime.status === 'idle' || $playtime.status === 'loading') && lastPlayed === null);

  onMount(() => {
    void playtime.load();
    const timer = setInterval(() => void playtime.load(), 30000);
    return () => clearInterval(timer);
  });
</script>

<div class="flex h-full min-h-0 flex-col gap-2.5">
  <StateBlock status={$games.status} hasData={$games.data.length > 0} error={$games.error} onRetry={() => void games.load()} />
  {#if $playtime.error !== null}<ErrorBanner message={`Tempo di gioco: ${$playtime.error}`} onRetry={() => void playtime.load()} />{/if}
  {#if $launchStates.error !== null}<ErrorBanner message={`Stato dei giochi: ${$launchStates.error}`} onRetry={() => void launchStates.load()} />{/if}
  {#if $launchError !== null}<ErrorBanner message={$launchError} />{/if}

  <div class="grid min-h-0 flex-1 gap-2.5 md:grid-cols-[minmax(0,1fr)_18rem]">
    <div class="flex min-h-0 min-w-0 flex-col">
      {#if lastPlayed !== null}
        {#key lastPlayed.game.id}
          <LastPlayedHero game={lastPlayed.game} onSettings={() => settingsGameId = lastPlayed?.game.id ?? null} />
        {/key}
      {:else if loading}
        <StateBlock status="loading" error={null} loadingMessage="Caricamento dell'ultima partita..." />
      {:else}
        <section class="flex min-h-0 flex-1 flex-col items-start justify-center gap-4 overflow-hidden rounded-2xl bg-zinc-900 p-8 light:bg-zinc-100">
          <Icon name="play" size="size-10 text-legio-activity" />
          <h1 class="text-3xl font-semibold text-zinc-50 light:text-zinc-900">La tua prossima partita ti aspetta</h1>
          <p class="max-w-xl text-sm text-zinc-400 light:text-zinc-600">Avvia un gioco dalla libreria. Qui troverai l'ultima partita e la tua attività di gioco.</p>
          <Button label="Apri la libreria" variant="secondary" onClick={() => selectSection('library')} />
        </section>
      {/if}
    </div>

    <div class="grid min-h-0 min-w-0 grid-rows-2 gap-2.5">
      <ActivityCalendar />
      <section class="min-h-0 rounded-2xl bg-zinc-900 p-4 light:bg-zinc-100" aria-labelledby="news-title">
        <h2 id="news-title" class="text-lg font-medium text-zinc-50 light:text-zinc-900">News</h2>
      </section>
    </div>
  </div>

  <section class="rounded-2xl bg-zinc-900 p-4 light:bg-zinc-100" aria-labelledby="recent-title">
    <div class="mb-2.5 flex flex-wrap items-center justify-between gap-2.5">
      <h2 id="recent-title" class="text-lg font-medium text-zinc-50 light:text-zinc-900">Giocati di recente</h2>
      <Button label="Libreria" variant="secondary" onClick={() => selectSection('library')} />
    </div>
    {#if recent.length > 1}
      <ul class="grid grid-cols-2 gap-2.5 sm:grid-cols-3">
        {#each recent.slice(1, 4) as item (item.game.id)}
          <GameCard
            game={item.game}
            playtimeMilliseconds={item.summary.totalMilliseconds}
            lastPlayedAt={item.summary.lastPlayedAt}
            artworkAsset="library_header"
            showLogo={false}
            launch={$launchStateByGame.get(item.game.id)}
            onOpen={() => openGame(item.game.id)}
          />
        {/each}
      </ul>
    {:else}
      <p class="py-4 text-sm text-zinc-400 light:text-zinc-600">Gli altri giochi avviati di recente compariranno qui.</p>
    {/if}
  </section>
</div>

{#if settingsGame !== null}
  <GameSettingsDialog game={settingsGame} onClose={() => settingsGameId = null} onRemoved={() => settingsGameId = null} />
{/if}
{#if $accountSwitchGame !== null}
  <Dialog open title="Cambio account Steam" onClose={dismissAccountSwitch}>
    <p class="text-sm text-zinc-300 light:text-zinc-700">Steam deve essere chiuso e riavviato per usare l'account salvato di {$accountSwitchGame.name}. Procedere?</p>
    <div class="flex justify-end gap-2">
      <Button label="Annulla" variant="secondary" onClick={dismissAccountSwitch} />
      <Button label="Riavvia e avvia" onClick={() => void confirmAccountSwitch()} />
    </div>
  </Dialog>
{/if}
