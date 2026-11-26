<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import type { PlaytimeSummary } from "../../services/playtime";
  import { gameUpdates } from "../../stores/game-updates";
  import { formatRelativeTime } from "../../utils/format";
  import { heroStats } from "./home-model";
  import { gameArtworkRevision, acquireGameArtwork } from "../../services/game-artwork";
  import { ensureSteamDetails, loadSteamDetails, steamDetails } from "../../stores/steam-details";
  import { abortGameLaunch, cancelPendingGameId, launchStateByGame, pendingGameId, playGame, stopGameProcess } from "../../stores/launch";
  import { openGame } from "../../stores/navigation";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import GameLaunchControls from "../library/GameLaunchControls.svelte";
  import GameLogo from "../library/GameLogo.svelte";
  import SteamArtwork from "../library/SteamArtwork.svelte";

  let { game, summary, onSettings }: { game: Game; summary: PlaytimeSummary; onSettings: () => void } = $props();
  const artworkGameId = $derived(game.id);
  const steamAppId = $derived(game.steamAppId);
  const detailsState = $derived(steamAppId === null ? null : $steamDetails[steamAppId]);
  const launch = $derived($launchStateByGame.get(game.id));
  const status = $derived(launch?.status ?? "idle");
  const hasUpdate = $derived($gameUpdates.data.some((update) => update.gameId === game.id));
  let now = $state(Date.now());
  const stats = $derived(heroStats(summary, (time) => t("Ultima partita: {0}", $language, [formatRelativeTime(time, now, $language)])));
  let bannerUrl = $state<string | null>(null);
  let artworkError = $state<string | null>(null);

  $effect(() => {
    const timer = setInterval(() => { now = Date.now(); }, 60000);
    return () => clearInterval(timer);
  });
  $effect(() => { void $language; if (steamAppId !== null) return ensureSteamDetails(steamAppId); });
  $effect(() => {
    const id = artworkGameId;
    void $gameArtworkRevision[`${id}:banner`];
    const artwork = acquireGameArtwork(id, "banner");
    let cancelled = false;
    bannerUrl = artwork.url;
    artworkError = null;
    void artwork.ready.then((url) => { if (!cancelled) bannerUrl = url; },
      (error: unknown) => { if (!cancelled) artworkError = toMessage(error); });
    return () => { cancelled = true; artwork.release(); };
  });
</script>

<section class="@container relative isolate flex min-h-0 flex-1 flex-col justify-end overflow-hidden rounded-panel bg-zinc-800 p-4 @min-[30rem]:p-6" aria-label={game.name}>
  <div class="absolute inset-0 -z-20 bg-gradient-to-br from-zinc-700 to-zinc-950">
    {#if bannerUrl !== null}
      <img src={bannerUrl} alt="" class="size-full scale-110 object-cover blur-md" />
    {:else if steamAppId !== null}
      <SteamArtwork {steamAppId} asset="hero" fallbackAsset="header" caption={false} class="size-full scale-110 object-cover blur-md" />
    {/if}
  </div>
  <div class="absolute inset-0 -z-10 bg-gradient-to-t from-black/80 via-black/30 to-transparent"></div>
  <div class="flex min-w-0 flex-col items-start gap-2 text-white">
    <div class="flex flex-wrap items-center gap-2">
      <p class="text-xs font-medium uppercase tracking-wide text-white/70">{t("Continua a giocare", $language)}</p>
      {#if status === "running"}
        <span class="rounded-full bg-legio-play px-2 py-0.5 text-xs font-medium text-white">{t("In esecuzione", $language)}</span>
      {/if}
      {#if hasUpdate}
        <span class="rounded-full bg-legio-download px-2 py-0.5 text-xs font-medium text-white">{t("Aggiornamento disponibile", $language)}</span>
      {/if}
    </div>
    <h2 class="sr-only">{game.name}</h2>
    <GameLogo {game} class="h-16 w-auto max-w-full object-contain object-left @min-[30rem]:h-24" />
    <p class="flex flex-wrap gap-x-2 text-sm text-white/70">
      {#each stats as part, index (index)}
        {#if index > 0}<span aria-hidden="true">&middot;</span>{/if}
        <span>{part}</span>
      {/each}
    </p>
  </div>
  <div class="mt-4 flex shrink-0 items-center gap-2">
    <GameLaunchControls {game} {launch} variant="primary" size="hero" menuAbove class="min-w-0 flex-1 @min-[30rem]:w-60 @min-[30rem]:flex-none"
      actionPending={$pendingGameId === game.id} cancelPending={$cancelPendingGameId === game.id}
      onPlay={playGame} onCancel={abortGameLaunch} onStop={stopGameProcess} />
    <Button label={t("Dettagli", $language)} variant="overlay" size="hero" class="hidden @min-[30rem]:inline-flex" onClick={() => openGame(game.id)} />
    <Button label={t("Dettagli", $language)} variant="overlay" size="hero" iconOnly class="@min-[30rem]:hidden" onClick={() => openGame(game.id)}><Icon name="info" size="xl" /></Button>
    <Button label={t("Impostazioni di {0}", $language, [game.name])} variant="overlay" size="hero" iconOnly onClick={onSettings}><Icon name="settings" size="xl" /></Button>
  </div>
</section>
{#if artworkError !== null}<ErrorBanner message={t("Immagine del gioco: {0}", $language, [artworkError])} />{/if}
{#if detailsState?.error && steamAppId !== null}
  <ErrorBanner message={t("Dettagli del gioco: {0}", $language, [detailsState.error])} onRetry={() => { if (steamAppId !== null) void loadSteamDetails(steamAppId, false).catch(() => undefined); }} />
{/if}
