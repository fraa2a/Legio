<script lang="ts">
  import { formatBytes, formatDate, formatDateTime } from "../../utils/format";
  import { games } from "../../stores/games";
  import {
    abortGameLaunch,
    accountSwitchGame,
    cancelPendingGameId,
    dismissAccountSwitch,
    confirmAccountSwitch,
    launchError,
    launchStateByGame,
    pendingGameId,
    playGame,
    stopGameProcess,
  } from "../../stores/launch";
  import { closeGame, selectSection, selectedGameId, type StoreGameSelection } from "../../stores/navigation";
  import { downloads, queueJob } from "../../stores/downloads";
  import { refreshSource, source, sourceRefreshError } from "../../stores/source";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import GameSettingsDialog from "./GameSettingsDialog.svelte";
  import GameLaunchControls from "./GameLaunchControls.svelte";
  import ArtworkViewer from "./ArtworkViewer.svelte";
  import SteamMetadata from "./SteamMetadata.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";
  import SystemRequirements from "./SystemRequirements.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import { getGameBanner, getGameIcon, gameArtworkRevision } from "../../services/game-artwork";
  import { toMessage } from "../../utils/errors";
  import { sourceStatusFor } from "../store/source-status";

  let { storeGame = null }: { storeGame?: StoreGameSelection | null } = $props();

  const game = $derived(storeGame === null ? ($games.data.find((entry) => entry.id === $selectedGameId) ?? null) : null);
  const steamAppId = $derived(storeGame?.steamAppId ?? game?.steamAppId ?? null);
  const name = $derived(storeGame?.name ?? game?.name ?? "");
  const launch = $derived(game === null ? undefined : $launchStateByGame.get(game.id));
  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const status = $derived(steamAppId === null ? null : sourceStatusFor($source.data.manifest, steamAppId));
  const entry = $derived(status?.entry ?? null);
  const job = $derived(storeGame === null ? null : ($downloads.data.find((candidate) => candidate.steamAppId === steamAppId) ?? null));
  const versionName = $derived(storeGame !== null || (game !== null && game.steamInstallPath !== null) ? "Steam" : "Legio");
  const lastPlayedLabel = $derived(storeGame === null && game !== null ? "Last played: Non disponibile" : null);

  let artworkOpen = $state(false);
  let gameSettingsOpen = $state(false);
  let customBannerUrl = $state<string | null>(null);
  let customIconUrl = $state<string | null>(null);
  let artworkError = $state<string | null>(null);
  let confirmOpen = $state(false);
  let queueing = $state(false);
  let queueError = $state<string | null>(null);

  $effect(() => {
    if (storeGame === null && $selectedGameId !== null && game === null) closeGame();
  });

  $effect(() => {
    if (steamAppId !== null) ensureSteamDetails(steamAppId);
  });

  $effect(() => {
    const currentGame = game;
    void $gameArtworkRevision;
    if (currentGame === null) return;

    let cancelled = false;
    let bannerObjectUrl: string | null = null;
    let iconObjectUrl: string | null = null;
    customBannerUrl = null;
    customIconUrl = null;
    artworkError = null;

    const asUrl = (bytes: number[], contentType: string): string =>
      URL.createObjectURL(new Blob([Uint8Array.from(bytes)], { type: contentType }));
    void Promise.all([getGameBanner(currentGame.id), getGameIcon(currentGame.id)]).then(
      ([banner, icon]) => {
        if (cancelled) return;
        bannerObjectUrl = banner === null ? null : asUrl(banner.bytes, banner.contentType);
        iconObjectUrl = icon === null ? null : asUrl(icon.bytes, icon.contentType);
        customBannerUrl = bannerObjectUrl;
        customIconUrl = iconObjectUrl;
      },
      (cause: unknown) => {
        if (!cancelled) artworkError = toMessage(cause);
      },
    );

    return () => {
      cancelled = true;
      if (bannerObjectUrl !== null) URL.revokeObjectURL(bannerObjectUrl);
      if (iconObjectUrl !== null) URL.revokeObjectURL(iconObjectUrl);
    };
  });

  async function startDownload(acceptUnverified: boolean): Promise<void> {
    if (steamAppId === null) return;
    queueError = null;
    queueing = true;
    try {
      await queueJob(steamAppId, acceptUnverified);
    } catch (error) {
      queueError = toMessage(error);
    } finally {
      queueing = false;
    }
  }

  function requestDownload(): void {
    if (status?.availability === "unverified") {
      confirmOpen = true;
      return;
    }
    void startDownload(false);
  }
</script>

{#if storeGame === null && game === null}
  <p class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
    Il gioco non è più disponibile.
  </p>
{:else}
  <div class="flex min-h-full flex-col gap-4">
    <section class="relative isolate w-full min-w-[1024px] overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200 {customBannerUrl !== null ? 'aspect-[2.2/1]' : 'aspect-[3.1/1]'}">
      {#if customBannerUrl !== null}
        <img src={customBannerUrl} alt="Banner personalizzato di {name}" class="absolute inset-0 block size-full object-cover object-center" />
      {:else if steamAppId !== null && (storeGame !== null || details !== null)}
        <SteamArtwork
          {steamAppId}
          asset="hero"
          fallbackAsset="header"
          version={detailsState?.cachedAt ?? null}
          caption={false}
          class="absolute inset-0 block size-full object-cover object-center"
        >
          {#snippet placeholder()}{@render backdrop()}{/snippet}
        </SteamArtwork>
      {:else}
        {@render backdrop()}
      {/if}
      {#if details !== null && customBannerUrl === null}
        <button type="button" class="absolute inset-0 z-0 cursor-zoom-in" aria-label="Ingrandisci copertina" onclick={() => (artworkOpen = true)}></button>
      {/if}
      <div class="pointer-events-none absolute inset-0 z-[1] bg-gradient-to-t from-zinc-950/95 via-zinc-950/35 to-transparent"></div>
      <div class="absolute inset-x-0 bottom-0 z-10 flex flex-nowrap items-end justify-between gap-4 pb-5 pl-5 pr-7">
        <div class="flex flex-nowrap items-end gap-2">
          {#if storeGame !== null}
            {#if job !== null}
              <Button label="VAI AI DOWNLOAD" variant="download" class="h-[60px] w-[240px] shrink-0 rounded-xl px-4 text-xl font-bold" onClick={() => selectSection("downloads")}>
                <Icon name="download" size="h-6 w-6" />
              </Button>
            {:else if status?.availability === "verified" || status?.availability === "unverified"}
              <Button label="SCARICA" variant="download" class="h-[60px] w-[240px] shrink-0 rounded-xl px-4 text-xl font-bold" disabled={queueing} onClick={requestDownload}>
                <Icon name="download" size="h-6 w-6" />
              </Button>
            {/if}
          {:else if game !== null}
            <GameLaunchControls
              {game}
              {launch}
              variant="primary"
              class="h-[60px] w-[240px] shrink-0 rounded-xl px-4 text-xl font-bold"
              actionPending={$pendingGameId === game.id}
              cancelPending={$cancelPendingGameId === game.id}
              onPlay={playGame}
              onCancel={abortGameLaunch}
              onStop={stopGameProcess}
            />
          {/if}
          <button
            type="button"
            disabled
            aria-label={`Versione ${versionName}${lastPlayedLabel === null ? "" : `, ${lastPlayedLabel}`}`}
            class="flex h-11 w-[230px] shrink-0 items-center gap-2 rounded-[10px] border border-white/10 bg-white/15 px-2.5 text-left text-zinc-100 light:bg-zinc-200"
          >
            {#if customIconUrl !== null}
              <img src={customIconUrl} alt="" class="size-8 shrink-0 rounded-md object-cover" />
            {:else if steamAppId !== null}
              <SteamArtwork {steamAppId} asset="capsule" version={detailsState?.cachedAt ?? null} caption={false} alt="" class="size-8 shrink-0 rounded-md object-cover">
                {#snippet placeholder()}<span class="flex size-8 shrink-0 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold text-zinc-300">{name.trim().charAt(0).toUpperCase() || "?"}</span>{/snippet}
              </SteamArtwork>
            {:else}
              <span class="flex size-8 shrink-0 items-center justify-center rounded-md bg-zinc-700 text-sm font-semibold text-zinc-300">{name.trim().charAt(0).toUpperCase() || "?"}</span>
            {/if}
            <span class="flex min-w-0 flex-1 flex-col gap-0.5">
              <span class="truncate text-base font-semibold leading-5">{versionName}</span>
              {#if lastPlayedLabel !== null}<span class="truncate text-[11px] leading-[14px] text-zinc-400">{lastPlayedLabel}</span>{/if}
            </span>
            <Icon name="chevron-down" size="h-4 w-4 shrink-0 text-zinc-400" />
          </button>
          {#if game !== null}
            <Button label="Impostazioni del gioco" square variant="secondary" class="shrink-0 border border-white/10 !bg-white/15 hover:!bg-white/20 light:!bg-zinc-200 light:hover:!bg-zinc-300" onClick={() => (gameSettingsOpen = true)}>
              <Icon name="settings" size="h-6 w-6" />
            </Button>
          {/if}
        </div>
        <div class="pointer-events-none flex h-[120px] w-[420px] shrink-0 flex-col items-end justify-end">
          <h2 class="flex h-full w-full items-end justify-end text-[22px] font-semibold leading-tight text-white">
            {#if customIconUrl !== null}
              <img src={customIconUrl} alt="Icona personalizzata di {name}" class="size-full object-contain object-right" />
            {:else if steamAppId !== null}
              <SteamArtwork {steamAppId} asset="logo" version={detailsState?.cachedAt ?? null} caption={false} alt="Logo di {name}" class="size-full object-contain object-right">
                {#snippet placeholder()}<span class="block max-w-full truncate text-right">{name}</span>{/snippet}
              </SteamArtwork>
            {:else}
              <span class="block max-w-full truncate text-right">{name}</span>
            {/if}
          </h2>
          {#if storeGame === null && launch?.error}<p class="mt-1 max-w-full truncate text-right text-sm text-red-300" role="alert">{launch.error}</p>{/if}
        </div>
      </div>
    </section>

    {#if storeGame !== null && queueError !== null}
      <ErrorBanner message={queueError} />
    {/if}
    {#if storeGame !== null && $sourceRefreshError !== null}
      <ErrorBanner message={$sourceRefreshError} onRetry={() => void refreshSource()} retryLabel="Riprova" />
    {/if}
    {#if storeGame === null && artworkError !== null}
      <ErrorBanner message={artworkError} />
    {/if}

    {#if storeGame === null && $launchError !== null}
      <ErrorBanner message={$launchError} />
    {/if}

    <div class="grid flex-1 gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
      <div class="min-w-0">
        {#if steamAppId !== null}<SteamMetadata {steamAppId} />{/if}
      </div>
      <div class="flex min-w-0 flex-col gap-4">
        <SystemRequirements
          requirements={details?.systemRequirements ?? null}
          loading={detailsState?.status === "loading"}
          hasSteamAppId={steamAppId !== null}
        />
        <Panel title="Info gioco">
          <div class="flex flex-col gap-3">
            <dl class="grid gap-2 text-sm">
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Versione</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry?.release.version ?? "non pubblicata"}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Pubblicato</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry === null ? "non pubblicato" : formatDate(entry.release.publishedAt)}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Dimensione</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry === null ? "non disponibile" : formatBytes(entry.download.sizeBytes)}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Manifest</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                  {entry === null || $source.data.cachedAt === null ? "non disponibile" : formatDateTime($source.data.cachedAt)}
                  {#if entry !== null && $source.data.stale}<span class="text-amber-300 light:text-amber-800"> · cache scaduta</span>{/if}
                </dd>
              </div>
            </dl>
            {#if storeGame !== null && $source.data.warning !== null}
              <p class="text-sm text-amber-300 light:text-amber-800">{$source.data.warning}</p>
            {/if}
          </div>
        </Panel>
      </div>
    </div>
  </div>
{/if}

{#if gameSettingsOpen && game !== null}
  <GameSettingsDialog
    {game}
    onClose={() => (gameSettingsOpen = false)}
    onRemoved={() => { gameSettingsOpen = false; closeGame(); }}
  />
{/if}

{#if artworkOpen && details !== null && steamAppId !== null}
  <ArtworkViewer
    {steamAppId}
    asset="hero"
    fallbackAsset="header"
    alt="Copertina di {name}"
    onClose={() => (artworkOpen = false)}
  />
{/if}

{#if storeGame !== null}
  <Dialog open={confirmOpen} title="Rilascio non verificato" onClose={() => (confirmOpen = false)}>
    <p class="text-sm text-zinc-300 light:text-zinc-700">
      Il rilascio di {name} non è stato verificato dallo staff Legio. Un archivio non verificato può contenere
      programmi dannosi. Procedere con il download e l'installazione?
    </p>
    <div class="flex justify-end gap-2">
      <Button label="Annulla" variant="secondary" onClick={() => (confirmOpen = false)} />
      <Button label="Scarica comunque" variant="danger" disabled={queueing} onClick={() => { confirmOpen = false; void startDownload(true); }} />
    </div>
  </Dialog>
{/if}

{#if storeGame === null && $accountSwitchGame !== null}
<Dialog
  open
  title="Cambio account Steam"
  onClose={dismissAccountSwitch}
>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Steam deve essere chiuso e riavviato per usare l'account salvato di {$accountSwitchGame?.name}.
    Procedere?
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Annulla" variant="secondary" onClick={dismissAccountSwitch} />
    <Button label="Riavvia e avvia" onClick={() => void confirmAccountSwitch()} />
  </div>
</Dialog>
{/if}

{#snippet backdrop()}
  <div
    class="absolute inset-0 bg-gradient-to-br from-zinc-700 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  ></div>
{/snippet}
