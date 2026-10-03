<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
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

  let { game, onSettings }: { game: Game; onSettings: () => void } = $props();
  const artworkGameId = $derived(game.id);
  const steamAppId = $derived(game.steamAppId);
  const detailsState = $derived(steamAppId === null ? null : $steamDetails[steamAppId]);
  const launch = $derived($launchStateByGame.get(game.id));
  let bannerUrl = $state<string | null>(null);
  let artworkError = $state<string | null>(null);

  $effect(() => { void $language; if (steamAppId !== null) ensureSteamDetails(steamAppId); });
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

<section class="relative isolate flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl bg-zinc-800 p-5 sm:p-6" aria-label={game.name}>
  <div class="absolute inset-0 -z-20 bg-gradient-to-br from-zinc-700 to-zinc-950">
    {#if bannerUrl !== null}
      <img src={bannerUrl} alt="" class="size-full scale-125 object-cover blur-xl" />
    {:else if steamAppId !== null}
      <SteamArtwork {steamAppId} asset="hero" fallbackAsset="header" caption={false} class="size-full scale-125 object-cover blur-xl" />
    {/if}
  </div>
  <div class="flex min-h-0 flex-1 items-center justify-center">
    <GameLogo {game} class="h-24 w-auto max-w-full object-contain object-center sm:h-32 lg:h-40" />
  </div>
  <div class="flex shrink-0 flex-wrap items-center justify-center gap-2">
    <Button label={t("Informazioni su {0}", $language, [game.name])} variant="secondary" circle onClick={() => openGame(game.id)} class="!bg-zinc-950/60 hover:!bg-zinc-950/75"><Icon name="info" /></Button>
    <GameLaunchControls {game} {launch} playLabel={t("Continua a giocare", $language)} variant="primary" class="rounded-full! px-5"
      actionPending={$pendingGameId === game.id} cancelPending={$cancelPendingGameId === game.id}
      onPlay={playGame} onCancel={abortGameLaunch} onStop={stopGameProcess} />
    <Button label={t("Impostazioni di {0}", $language, [game.name])} variant="secondary" circle onClick={onSettings} class="!bg-zinc-950/60 hover:!bg-zinc-950/75"><Icon name="settings" /></Button>
  </div>
</section>
{#if artworkError !== null}<ErrorBanner message={t("Immagine del gioco: {0}", $language, [artworkError])} />{/if}
{#if detailsState?.error && steamAppId !== null}
  <ErrorBanner message={t("Dettagli del gioco: {0}", $language, [detailsState.error])} onRetry={() => { if (steamAppId !== null) void loadSteamDetails(steamAppId, false).catch(() => undefined); }} />
{/if}
