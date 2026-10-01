<script lang="ts">
  import { onMount } from "svelte";
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
  import { closeGame, openGame, selectSection, selectedGameId, type StoreGameSelection } from "../../stores/navigation";
  import { downloads, queueJob } from "../../stores/downloads";
  import type { SourceEntry } from "../../services/legio-source";
  import { describeDownloadStatus } from "../../services/downloads";
  import { playtime } from "../../stores/playtime";
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
  import GameVersionSelect from "./GameVersionSelect.svelte";
  import { getGameBanner, getGameIcon, gameArtworkRevision } from "../../services/game-artwork";
  import { toMessage } from "../../utils/errors";
  import { sourceReleaseId, sourceReleasesFor } from "../store/source-status";

  let { storeGame = null }: { storeGame?: StoreGameSelection | null } = $props();

  const game = $derived(storeGame === null ? ($games.data.find((entry) => entry.id === $selectedGameId) ?? null) : null);
  const steamAppId = $derived(storeGame?.steamAppId ?? game?.steamAppId ?? null);
  const launch = $derived(game === null ? undefined : $launchStateByGame.get(game.id));
  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const releases = $derived(steamAppId === null ? [] : sourceReleasesFor($source.data.manifest, steamAppId));
  let selectedReleaseId = $state<string | null>(null);
  const status = $derived(releases.find((release) => sourceReleaseId(release.entry) === selectedReleaseId) ?? releases[0] ?? null);
  const entry = $derived(status?.entry ?? null);
  const name = $derived(storeGame !== null ? (entry?.name ?? storeGame.name) : (game?.name ?? ""));
  const job = $derived(storeGame === null || entry === null ? null : ($downloads.data.find((candidate) => candidate.steamAppId === steamAppId && candidate.url === entry.download.url && candidate.releaseVersion === entry.release.version && candidate.status !== "installed" && candidate.status !== "cancelled") ?? null));
  const downloadProgress = $derived(job === null || job.sizeBytes === 0 ? 0 : Math.min(100, Math.max(0, job.downloadedBytes / job.sizeBytes * 100)));
  const libraryVersions = $derived(game === null ? [] : $games.data.filter((candidate) => candidate.steamAppId === null ? candidate.id === game.id : candidate.steamAppId === game.steamAppId));
  const versionOptions = $derived(storeGame !== null
    ? releases.map((release) => ({ id: sourceReleaseId(release.entry), name: release.entry.name, subtitle: `Versione ${release.entry.release.version}` }))
    : libraryVersions.map((candidate) => {
      const lastPlayed = $playtime.data.find((summary) => summary.gameId === candidate.id)?.lastPlayedAt ?? null;
      return { id: candidate.id, name: candidate.name, subtitle: lastPlayed === null ? "Mai giocato" : `Ultima partita: ${new Date(lastPlayed).toLocaleString("it-IT", { dateStyle: "short", timeStyle: "short" })}`, isSteam: candidate.steamInstallPath !== null };
    }));
  const selectedVersionId = $derived(storeGame !== null ? (entry === null ? "" : sourceReleaseId(entry)) : (game?.id ?? ""));

  let artworkOpen = $state(false);
  let gameSettingsOpen = $state(false);
  let customBannerUrl = $state<string | null>(null);
  let customIconUrl = $state<string | null>(null);
  let artworkError = $state<string | null>(null);
  let confirmOpen = $state(false);
  let confirmEntry = $state<SourceEntry | null>(null);
  let queueing = $state(false);
  let queueError = $state<string | null>(null);

  onMount(() => {
    if (storeGame !== null) return;
    void playtime.load();
    const timer = setInterval(() => void playtime.load(), 30000);
    return () => clearInterval(timer);
  });

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

  async function startDownload(downloadUrl: string, releaseVersion: string, acceptUnverified: boolean): Promise<void> {
    if (steamAppId === null) return;
    queueError = null;
    queueing = true;
    try {
      await queueJob(steamAppId, downloadUrl, releaseVersion, acceptUnverified);
    } catch (error) {
      queueError = toMessage(error);
    } finally {
      queueing = false;
    }
  }

  function requestDownload(): void {
    if (entry === null) return;
    if (status?.availability === "unverified") {
      confirmEntry = entry;
      confirmOpen = true;
      return;
    }
    void startDownload(entry.download.url, entry.release.version, false);
  }

  function selectVersion(id: string): void {
    if (storeGame !== null) selectedReleaseId = id;
    else openGame(id);
  }
</script>

{#if storeGame === null && game === null}
  <p class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
    Il gioco non è più disponibile.
  </p>
{:else}
  <div class="flex min-h-full flex-col gap-4">
    <section class="relative isolate z-10 w-full min-w-[1024px] rounded-2xl bg-zinc-800 light:bg-zinc-200 {customBannerUrl !== null ? 'aspect-[2.2/1]' : 'aspect-[3.1/1]'}">
      <div class="absolute inset-0 overflow-hidden rounded-2xl">
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
      </div>
      <div class="absolute inset-x-0 bottom-0 z-10 flex flex-nowrap items-end justify-between gap-4 pb-5 pl-5 pr-7">
        <div class="flex flex-nowrap items-end gap-2">
          {#if storeGame !== null}
            {#if job !== null}
              <button type="button" class="relative flex h-[60px] w-[240px] shrink-0 items-center justify-center gap-2 overflow-hidden rounded-xl bg-legio-download px-4 text-xl font-bold text-white hover:bg-legio-download-hover" onclick={() => selectSection("downloads")}>
                <Icon name="download" size="h-6 w-6" />
                {job.status === "queued" || job.status === "downloading" ? "DOWNLOADING" : describeDownloadStatus(job.status).toUpperCase()}
                <span class="absolute inset-x-0 bottom-0 h-2 bg-white/30" role="progressbar" aria-label="Avanzamento download" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(downloadProgress)}>
                  <span class="block h-full bg-white" style:width={`${downloadProgress}%`}></span>
                </span>
              </button>
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
          {#if versionOptions.length > 0}
            <GameVersionSelect {steamAppId} options={versionOptions} selectedId={selectedVersionId} onSelect={selectVersion} />
          {/if}
          {#if game !== null}
            <Button label="Impostazioni del gioco" square variant="secondary" class="h-[60px] w-[60px] shrink-0 rounded-xl border border-white/10 !bg-zinc-950/60 hover:!bg-zinc-950/75 light:!border-zinc-900/10 light:!bg-zinc-100/70 light:hover:!bg-zinc-200" onClick={() => (gameSettingsOpen = true)}>
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
                {#snippet placeholder()}{@render nameText()}{/snippet}
              </SteamArtwork>
            {:else}
              {@render nameText()}
            {/if}
          </h2>
          {#if storeGame === null && launch?.error}<p class="mt-1 max-w-full truncate rounded-lg bg-zinc-950/60 px-3 py-1.5 text-right text-sm text-red-300 light:bg-zinc-100/70 light:text-red-700" role="alert">{launch.error}</p>{/if}
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
        {#if storeGame !== null}<Panel title="Info gioco">
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
        </Panel>{/if}
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
      <Button label="Scarica comunque" variant="danger" disabled={queueing} onClick={() => { confirmOpen = false; if (confirmEntry !== null) void startDownload(confirmEntry.download.url, confirmEntry.release.version, true); }} />
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

{#snippet nameText()}
  <span class="block max-w-full truncate rounded-lg bg-zinc-950/60 px-3 py-1.5 text-right text-zinc-100 light:bg-zinc-100/70 light:text-zinc-900">{name}</span>
{/snippet}

{#snippet backdrop()}
  <div
    class="absolute inset-0 bg-gradient-to-br from-zinc-700 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  ></div>
{/snippet}
