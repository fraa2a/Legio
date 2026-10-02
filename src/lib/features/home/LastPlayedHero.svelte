<script lang="ts">
  import type { Game } from "../../services/local-state";
  import type { PlaytimeSummary } from "../../services/playtime";
  import { gameArtworkRevision, getGameBanner } from "../../services/game-artwork";
  import { ensureSteamDetails, loadSteamDetails, steamDetails } from "../../stores/steam-details";
  import { abortGameLaunch, cancelPendingGameId, launchStateByGame, pendingGameId, playGame, stopGameProcess } from "../../stores/launch";
  import { openGame } from "../../stores/navigation";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import GameLaunchControls from "../library/GameLaunchControls.svelte";
  import SteamArtwork from "../library/SteamArtwork.svelte";
  import { formatPlaytime } from "./home-model";

  let { game, summary, onSettings }: { game: Game; summary: PlaytimeSummary; onSettings: () => void } = $props();
  const steamAppId = $derived(game.steamAppId);
  const detailsState = $derived(steamAppId === null ? null : $steamDetails[steamAppId]);
  const launch = $derived($launchStateByGame.get(game.id));
  let bannerUrl = $state<string | null>(null);
  let artworkError = $state<string | null>(null);

  $effect(() => { if (steamAppId !== null) ensureSteamDetails(steamAppId); });
  $effect(() => {
    const id = game.id;
    void $gameArtworkRevision;
    let cancelled = false;
    let objectUrl: string | null = null;
    bannerUrl = null;
    artworkError = null;
    void getGameBanner(id).then((banner) => {
      if (cancelled || banner === null) return;
      objectUrl = URL.createObjectURL(new Blob([Uint8Array.from(banner.bytes)], { type: banner.contentType }));
      bannerUrl = objectUrl;
    }, (error: unknown) => { if (!cancelled) artworkError = toMessage(error); });
    return () => {
      cancelled = true;
      if (objectUrl !== null) URL.revokeObjectURL(objectUrl);
    };
  });
</script>

<section class="relative isolate flex min-h-96 flex-col justify-between overflow-hidden rounded-2xl bg-zinc-800 p-6 sm:p-8 xl:aspect-[2.8/1]" aria-labelledby="last-played-title">
  <div class="absolute inset-0 -z-20 bg-gradient-to-br from-zinc-700 to-zinc-950">
    {#if bannerUrl !== null}
      <img src={bannerUrl} alt="" class="size-full object-cover" />
    {:else if steamAppId !== null}
      <SteamArtwork {steamAppId} asset="hero" fallbackAsset="header" caption={false} class="size-full object-cover" />
    {/if}
  </div>
  <div class="absolute inset-0 -z-10 bg-gradient-to-t from-black via-black/50 to-black/10"></div>
  <div>
    <p class="text-xs font-medium uppercase tracking-widest text-white/70">Ultima partita</p>
    {#if summary.lastPlayedAt !== null}
      <p class="mt-1 text-sm text-white/90"><time datetime={new Date(summary.lastPlayedAt).toISOString()}>{new Date(summary.lastPlayedAt).toLocaleDateString("it-IT", { day: "numeric", month: "long", year: "numeric" })}</time></p>
    {/if}
  </div>
  <div class="mt-16 flex flex-col gap-6 lg:flex-row lg:items-end lg:justify-between">
    <div class="min-w-0 max-w-2xl">
      <p class="mb-3 flex items-center gap-1.5 text-xs text-white/80"><Icon name="clock" size="size-4" />{formatPlaytime(summary.totalMilliseconds)} giocate{launch?.status === 'running' ? ' · In esecuzione' : ''}</p>
      <h1 id="last-played-title" class="text-3xl font-semibold text-white sm:text-4xl">{game.name}</h1>
      <p class="mt-3 line-clamp-3 text-sm leading-relaxed text-white/80">{detailsState?.details?.shortDescription ?? "Riprendi la tua partita e torna a esplorare il tuo gioco."}</p>
    </div>
    <div class="flex shrink-0 flex-wrap items-center gap-2">
      <Button label={`Impostazioni di ${game.name}`} variant="secondary" circle onClick={onSettings}><Icon name="settings" /></Button>
      <Button label={`Informazioni su ${game.name}`} variant="secondary" circle onClick={() => openGame(game.id)}><Icon name="info" /></Button>
      <GameLaunchControls {game} {launch} playLabel="Continue playing" variant="primary" class="rounded-full! px-5"
        actionPending={$pendingGameId === game.id} cancelPending={$cancelPendingGameId === game.id}
        onPlay={playGame} onCancel={abortGameLaunch} onStop={stopGameProcess} />
    </div>
  </div>
</section>
{#if artworkError !== null}<ErrorBanner message={`Immagine del gioco: ${artworkError}`} />{/if}
{#if detailsState?.error && steamAppId !== null}
  <ErrorBanner message={`Dettagli del gioco: ${detailsState.error}`} onRetry={() => { if (steamAppId !== null) void loadSteamDetails(steamAppId, false).catch(() => undefined); }} />
{:else if detailsState?.stale}
  <p class="text-xs text-zinc-500">Descrizione disponibile dalla cache locale.</p>
{/if}
