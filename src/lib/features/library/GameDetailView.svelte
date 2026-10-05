<script lang="ts">
  import { t, language } from "../../i18n";
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
  import AddGameDialog from "./AddGameDialog.svelte";
  import { acquireGameArtwork, gameArtworkRevision } from "../../services/game-artwork";
  import { toMessage } from "../../utils/errors";
  import { sourceReleaseId, sourceReleasesFor } from "../store/source-status";

  let { storeGame = null, gameId = null }: { storeGame?: StoreGameSelection | null; gameId?: string | null } = $props();

  const selectedGame = $derived(gameId ?? $selectedGameId);
  const game = $derived(storeGame === null ? ($games.data.find((entry) => entry.id === selectedGame) ?? null) : null);
  const artworkGameId = $derived(game?.id ?? null);
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
    ? releases.map((release) => ({ id: sourceReleaseId(release.entry), name: release.entry.name, subtitle: t("Versione {0}", $language, [release.entry.release.version]) }))
    : libraryVersions.map((candidate) => {
      const lastPlayed = $playtime.data.find((summary) => summary.gameId === candidate.id)?.lastPlayedAt ?? null;
      return { id: candidate.id, name: candidate.name, subtitle: lastPlayed === null ? t("Mai giocato", $language) : t("Ultima partita: {0}", $language, [new Date(lastPlayed).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" })]), isSteam: candidate.steamInstallPath !== null };
    }));
  const selectedVersionId = $derived(storeGame !== null ? (entry === null ? "" : sourceReleaseId(entry)) : (game?.id ?? ""));

  let artworkOpen = $state(false);
  let addVersionOpen = $state(false);
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
    void $language; if (steamAppId !== null) ensureSteamDetails(steamAppId);
  });

  $effect(() => {
    const id = artworkGameId;
    if (id === null) { customBannerUrl = null; customIconUrl = null; return; }
    void $gameArtworkRevision[`${id}:banner`];
    void $gameArtworkRevision[`${id}:icon`];
    const banner = acquireGameArtwork(id, "banner");
    const icon = acquireGameArtwork(id, "icon");
    let cancelled = false;
    customBannerUrl = banner.url;
    customIconUrl = icon.url;
    artworkError = null;
    void Promise.all([banner.ready, icon.ready]).then(([bannerUrl, iconUrl]) => {
      if (cancelled) return;
      customBannerUrl = bannerUrl;
      customIconUrl = iconUrl;
    }, (cause: unknown) => { if (!cancelled) artworkError = toMessage(cause); });
    return () => { cancelled = true; banner.release(); icon.release(); };
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
  <p class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">{t("\n    Il gioco non è più disponibile.\n  ", $language)}</p>
{:else}
  <div class="flex min-h-full flex-col gap-4">
    <section class="relative isolate z-10 aspect-[1920/620] w-full min-h-[20rem] min-w-[1024px] overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200">
      {#if customBannerUrl !== null}
        <img src={customBannerUrl} alt="{t("Banner personalizzato di ", $language)}{name}" class="absolute inset-0 size-full object-cover" />
      {:else if steamAppId !== null && (storeGame !== null || details !== null)}
        <SteamArtwork
          {steamAppId}
          asset="hero"
          fallbackAsset="header"
          version={detailsState?.cachedAt ?? null}
          caption={false}
          class="absolute inset-0 size-full object-cover"
        >
          {#snippet placeholder()}{@render backdrop()}{/snippet}
        </SteamArtwork>
      {:else}
        {@render backdrop()}
      {/if}
      {#if details !== null && customBannerUrl === null}
        <button type="button" class="absolute inset-0 z-0 cursor-zoom-in" aria-label={t("Ingrandisci copertina", $language)} onclick={() => (artworkOpen = true)}></button>
      {/if}
      <div class="absolute inset-x-0 bottom-0 z-10 flex flex-nowrap items-end justify-between gap-4 pb-5 pl-5 pr-7">
        <div class="flex flex-nowrap items-end gap-2">
          {#if storeGame !== null}
            {#if job !== null}
              <button type="button" class="relative flex h-[60px] w-[240px] shrink-0 items-center justify-center gap-2 overflow-hidden rounded-xl bg-legio-download px-4 text-xl font-bold text-white hover:bg-legio-download-hover" onclick={() => selectSection("downloads")}>
                <Icon name="download" size="h-6 w-6" />
                {job.status === "queued" || job.status === "downloading" ? t("SCARICAMENTO", $language) : describeDownloadStatus(job.status).toUpperCase()}
                <span class="absolute inset-x-0 bottom-0 h-2 bg-white/30" role="progressbar" aria-label={t("Avanzamento download", $language)} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(downloadProgress)}>
                  <span class="block h-full bg-white" style:width={`${downloadProgress}%`}></span>
                </span>
              </button>
            {:else if status?.availability === "verified" || status?.availability === "unverified"}
              <Button label={t("SCARICA", $language)} variant="download" class="h-[60px] w-[240px] shrink-0 rounded-xl px-4 text-xl font-bold" disabled={queueing} onClick={requestDownload}>
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
            <GameVersionSelect {steamAppId} options={versionOptions} selectedId={selectedVersionId} onSelect={selectVersion} onAdd={game !== null && libraryVersions.length === 1 && steamAppId !== null ? () => { addVersionOpen = true; } : undefined} />
          {/if}
          {#if game !== null}
            <Button label={t("Impostazioni del gioco", $language)} square variant="secondary" class="h-[60px] w-[60px] shrink-0 rounded-xl border border-white/10 !bg-zinc-950/60 hover:!bg-zinc-950/75 light:!border-zinc-900/10 light:!bg-zinc-100/70 light:hover:!bg-zinc-200" onClick={() => (gameSettingsOpen = true)}>
              <Icon name="settings" size="h-6 w-6" />
            </Button>
          {/if}
        </div>
        <div class="pointer-events-none flex h-[120px] w-[420px] shrink-0 flex-col items-end justify-end">
          <h2 class="flex h-full w-full items-end justify-end text-[22px] font-semibold leading-tight text-white">
            {#if customIconUrl !== null}
              <img src={customIconUrl} alt="{t("Icona personalizzata di ", $language)}{name}" class="size-full object-contain object-right" />
            {:else if steamAppId !== null}
              <SteamArtwork {steamAppId} asset="logo" version={detailsState?.cachedAt ?? null} caption={false} alt="{t("Logo di ", $language)}{name}" class="size-full object-contain object-right">
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
      <ErrorBanner message={$sourceRefreshError} onRetry={() => void refreshSource()} retryLabel={t("Riprova", $language)} />
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
        {#if storeGame !== null}<Panel title={t("Info gioco", $language)}>
          <div class="flex flex-col gap-3">
            <dl class="grid gap-2 text-sm">
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">{t("Versione", $language)}</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry?.release.version ?? t("non pubblicata", $language)}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">{t("Pubblicato", $language)}</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry === null ? t("non pubblicato", $language) : formatDate(entry.release.publishedAt)}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">{t("Dimensione", $language)}</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry === null ? t("non disponibile", $language) : formatBytes(entry.download.sizeBytes)}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Manifest</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                  {entry === null || $source.data.cachedAt === null ? t("non disponibile", $language) : formatDateTime($source.data.cachedAt)}
                  {#if entry !== null && $source.data.stale}<span class="text-amber-300 light:text-amber-800">{t(" · cache scaduta", $language)}</span>{/if}
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
    alt="{t("Copertina di ", $language)}{name}"
    onClose={() => (artworkOpen = false)}
  />
{/if}

{#if addVersionOpen && game !== null && steamAppId !== null}
  <AddGameDialog prefill={{ steamAppId, name: details?.name ?? game.name, suggestedName: `${game.name} (2)` }} onClose={() => (addVersionOpen = false)} />
{/if}

{#if storeGame !== null}
  <Dialog open={confirmOpen} title={t("Rilascio non verificato", $language)} onClose={() => (confirmOpen = false)}>
  {#snippet children(dismiss)}
    <p class="text-sm text-zinc-300 light:text-zinc-700">{t("\n      Il rilascio di ", $language)}{name}{t(" non è stato verificato dallo staff Legio. Un archivio non verificato può contenere\n      programmi dannosi. Procedere con il download e l'installazione?\n    ", $language)}</p>
    <div class="flex justify-end gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" onClick={dismiss} />
      <Button label={t("Scarica comunque", $language)} variant="danger" disabled={queueing} onClick={() => { confirmOpen = false; if (confirmEntry !== null) void startDownload(confirmEntry.download.url, confirmEntry.release.version, true); }} />
    </div>
    {/snippet}
</Dialog>
{/if}

{#if storeGame === null && $accountSwitchGame !== null}
<Dialog
  open
  title={t("Cambio account Steam", $language)}
  onClose={dismissAccountSwitch}
>
  {#snippet children(dismiss)}
  <p class="text-sm text-zinc-300 light:text-zinc-700">{t("\n    Steam deve essere chiuso e riavviato per usare l'account salvato di ", $language)}{$accountSwitchGame?.name}{t(".\n    Procedere?\n  ", $language)}</p>
  <div class="flex justify-end gap-2">
    <Button label={t("Annulla", $language)} variant="secondary" onClick={dismiss} />
    <Button label={t("Riavvia e avvia", $language)} onClick={() => void confirmAccountSwitch()} />
  </div>
  {/snippet}
</Dialog>
{/if}

{#snippet nameText()}
  <span class="block max-w-full truncate rounded-lg bg-zinc-950/60 px-3 py-1.5 text-right text-zinc-100 light:bg-zinc-100/70 light:text-zinc-900">{name}</span>
{/snippet}

{#snippet backdrop()}
  <div
    class="min-h-[20rem] bg-gradient-to-br from-zinc-700 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  ></div>
{/snippet}
