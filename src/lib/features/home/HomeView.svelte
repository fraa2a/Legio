<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { describeDownloadStatus, downloadProgressPercent } from "../../services/downloads";
  import { downloads, orderedDownloads } from "../../stores/downloads";
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

<div class="flex flex-col gap-5 p-4 sm:p-6">
  <StateBlock status={$games.status} hasData={$games.data.length > 0} error={$games.error} onRetry={() => void games.load()} />
  {#if $playtime.error !== null}<ErrorBanner message={`Tempo di gioco: ${$playtime.error}`} onRetry={() => void playtime.load()} />{/if}
  {#if $launchStates.error !== null}<ErrorBanner message={`Stato dei giochi: ${$launchStates.error}`} onRetry={() => void launchStates.load()} />{/if}
  {#if $launchError !== null}<ErrorBanner message={$launchError} />{/if}

  {#if lastPlayed !== null}
    {#key lastPlayed.game.id}
      <LastPlayedHero game={lastPlayed.game} summary={lastPlayed.summary} onSettings={() => settingsGameId = lastPlayed?.game.id ?? null} />
    {/key}
  {:else if loading}
    <StateBlock status="loading" error={null} loadingMessage="Caricamento dell'ultima partita..." />
  {:else}
    <section class="flex min-h-72 flex-col items-start justify-center gap-4 rounded-2xl bg-zinc-900 p-8 light:bg-zinc-100">
      <Icon name="play" size="size-10 text-legio-activity" />
      <h1 class="text-3xl font-semibold text-zinc-50 light:text-zinc-900">La tua prossima partita ti aspetta</h1>
      <p class="max-w-xl text-sm text-zinc-400 light:text-zinc-600">Avvia un gioco dalla libreria. Qui troverai l'ultima partita e la tua attività di gioco.</p>
      <Button label="Apri la libreria" variant="secondary" onClick={() => selectSection('library')} />
    </section>
  {/if}

  <div class="grid items-start gap-5 xl:grid-cols-[minmax(0,1fr)_minmax(20rem,24rem)]">
    <div class="flex min-w-0 flex-col gap-5">
      <section class="rounded-2xl bg-zinc-900 p-5 light:bg-zinc-100" aria-labelledby="recent-title">
        <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
          <h2 id="recent-title" class="text-lg font-medium text-zinc-50 light:text-zinc-900">Giocati di recente</h2>
          <Button label="Libreria" variant="secondary" onClick={() => selectSection('library')} />
        </div>
        {#if recent.length > 1}
          <ul class="grid grid-cols-2 gap-3 sm:grid-cols-3">
            {#each recent.slice(1, 4) as item (item.game.id)}
              <li class="min-w-0">
                <ul><GameCard game={item.game} portrait playtimeMilliseconds={item.summary.totalMilliseconds} launch={$launchStateByGame.get(item.game.id)} onOpen={() => openGame(item.game.id)} /></ul>
                <p class="mt-2 truncate text-sm text-zinc-200 light:text-zinc-800">{item.game.name}</p>
                <p class="mt-1 text-xs text-zinc-500">{new Date(item.summary.lastPlayedAt ?? 0).toLocaleDateString('it-IT', { day: 'numeric', month: 'short' })}</p>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="py-6 text-sm text-zinc-400 light:text-zinc-600">Gli altri giochi avviati di recente compariranno qui.</p>
        {/if}
      </section>

      <section class="rounded-2xl bg-zinc-900 p-5 light:bg-zinc-100" aria-labelledby="downloads-title">
        <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
          <h2 id="downloads-title" class="text-lg font-medium text-zinc-50 light:text-zinc-900">I tuoi download</h2>
          <Button label="Apri download" variant="secondary" onClick={() => selectSection('downloads')} />
        </div>
        <StateBlock status={$downloads.status} hasData={$downloads.data.length > 0} emptyMessage="Nessun download in coda." error={$downloads.error} onRetry={() => void downloads.load()} />
        <ul class="flex flex-col gap-3">
          {#each $orderedDownloads.slice(0, 3) as job (job.id)}
            <li>
              <button type="button" class="w-full rounded-xl bg-white/5 p-3 text-left hover:bg-white/10 light:bg-zinc-200 light:hover:bg-zinc-300" onclick={() => selectSection('downloads')}>
                <span class="flex justify-between gap-3 text-sm"><span class="truncate font-medium text-zinc-200 light:text-zinc-800">{job.name}</span><span class="shrink-0 text-xs text-zinc-400 light:text-zinc-600">{describeDownloadStatus(job.status)}</span></span>
                <span class="mt-2 block h-1 overflow-hidden rounded-full bg-zinc-700 light:bg-zinc-300" role="progressbar" aria-label={`Download di ${job.name}`} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(downloadProgressPercent(job))}><span class="block h-full rounded-full bg-legio-download" style:width={`${downloadProgressPercent(job)}%`}></span></span>
                {#if job.error}<span class="mt-2 block text-xs text-red-300 light:text-red-700">{job.error}</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
      </section>
    </div>
    <ActivityCalendar />
  </div>
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
