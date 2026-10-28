<script lang="ts">
  import { artworkDemand } from "../../utils/artwork-demand";
  const demand = artworkDemand();
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import { startVisibleRefresh } from "../../stores/visible-refresh";
  import { formatBytes, formatDate, formatDateTime } from "../../utils/format";
  import { games } from "../../stores/games";
  import {
    abortGameLaunch,
    cancelPendingGameId,
    launchStateByGame,
    pendingGameId,
    playGame,
    stopGameProcess,
  } from "../../stores/launch";
  import { closeGame, openGame, selectSection, selectedGameId, type StoreGameSelection } from "../../stores/navigation";
  import { downloads, queueJob } from "../../stores/downloads";
  import type { SourceEntry } from "../../services/legio-source";
  import { describeDownloadStatus, downloadProgressPercent } from "../../services/downloads";
  import { playtime } from "../../stores/playtime";
  import { refreshSource, source, sourceRefreshError } from "../../stores/source";
  import { refreshSteamDetailsOnOpen, steamDetails } from "../../stores/steam-details";
  import { createArtworkWork } from "../../services/artwork-work";
  import { refreshSteamImagesOnOpen } from "../../services/steam-details";
  import Button from "../../components/ui/Button.svelte";
  import DownloadProgressButton from "../downloads/DownloadProgressButton.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import GameSettingsDialog from "./GameSettingsDialog.svelte";
  import GameUpdateButton from "./GameUpdateButton.svelte";
  import GameLaunchControls from "./GameLaunchControls.svelte";
  import ArtworkViewer from "./ArtworkViewer.svelte";
  import SteamMetadata from "./SteamMetadata.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";
  import SystemRequirements from "./SystemRequirements.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import DownloadMirrorSelect from "./DownloadMirrorSelect.svelte";
  import GameVersionSelect from "./GameVersionSelect.svelte";
  import AddGameDialog from "./AddGameDialog.svelte";
  import { acquireGameArtwork, gameArtworkRevision } from "../../services/game-artwork";
  import { toMessage } from "../../utils/errors";
  import { sourceNameFor, sourceReleaseId, sourceReleasesFor } from "../store/source-status";
  import { nextVersionAfterRemoval } from "./library-model";

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
  let selectedMirror = $state("auto");
  const mirrorUris = $derived(entry === null ? [] : ($source.data.sources.find(origin => origin.id === entry.sourceId)?.candidates?.[entry.id ?? ""] ?? [entry.download.url]));
  const job = $derived(storeGame === null || entry === null ? null : ($downloads.data.find((candidate) => candidate.steamAppId === steamAppId && (candidate.sourceId && candidate.sourceEntryId ? candidate.sourceId === entry.sourceId && candidate.sourceEntryId === entry.id : candidate.url === entry.download.url && candidate.releaseVersion === entry.release.version) && candidate.status !== "installed" && candidate.status !== "cancelled") ?? null));
  const downloadProgress = $derived(job === null ? 0 : downloadProgressPercent(job));
  const libraryVersions = $derived(game === null ? [] : $games.data.filter((candidate) => candidate.steamAppId === null ? candidate.id === game.id : candidate.steamAppId === game.steamAppId));
  const versionOptions = $derived(storeGame !== null
    ? releases.map((release) => ({ id: sourceReleaseId(release.entry), name: release.entry.name, sourceName: sourceNameFor(release.entry, $source.data.sources), subtitle: t("Versione {0}", $language, [release.entry.release.version]) }))
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
  let confirmPreferredUri = $state<string | null>(null);
  let queueing = $state(false);
  let queueError = $state<string | null>(null);

  onMount(() => {
    if (storeGame !== null) return;
    return startVisibleRefresh(() => void playtime.load(), 30000);
  });

  $effect(() => {
    if (storeGame === null && $selectedGameId !== null && game === null && !gameSettingsOpen) closeGame();
  });

  $effect(() => {
    void $language;
    if (steamAppId === null || !demand()) return;
    const work = createArtworkWork("visible");
    void refreshSteamDetailsOnOpen(steamAppId, work).catch(() => undefined);
    void refreshSteamImagesOnOpen(steamAppId, work).catch(() => undefined);
    return work.release;
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

  async function startDownload(downloadUrl: string, releaseVersion: string, acceptUnverified: boolean, preferredUri: string | null = selectedMirror === "auto" ? null : selectedMirror): Promise<void> {
    if (steamAppId === null) return;
    queueError = null;
    queueing = true;
    try {
      await queueJob(steamAppId, downloadUrl, releaseVersion, acceptUnverified, confirmEntry?.download.url === downloadUrl ? confirmEntry.sourceId : entry?.sourceId, preferredUri);
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
      confirmPreferredUri = selectedMirror === "auto" ? null : selectedMirror;
      confirmOpen = true;
      return;
    }
    void startDownload(entry.download.url, entry.release.version, false);
  }

  function selectVersion(id: string): void {
    selectedMirror = "auto";
    if (storeGame !== null) selectedReleaseId = id;
    else openGame(id);
  }
</script>

{#if storeGame === null && game === null}
  <p class="legio-glass rounded-xl p-6 text-zinc-400 light:text-zinc-600">{t("Il gioco non è più disponibile.", $language)}</p>
{:else}
  <div class="flex min-h-full flex-col gap-4">
    <section class="relative isolate aspect-[1920/620] min-h-80 w-full overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200">
      {#if customBannerUrl !== null}
        <img src={customBannerUrl} alt={t("Banner personalizzato di {0}", $language, [name])} class="absolute inset-0 size-full object-cover" />
      {:else if steamAppId !== null}
        <SteamArtwork
          {steamAppId}
          asset="hero"
          fallbackAsset="header"
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
      <div class="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex justify-end pr-7 pb-5 pl-5">
        <h2 class="flex h-30 w-105 max-w-full items-end justify-end text-2xl font-semibold leading-tight text-white">
          {#if customIconUrl !== null}
            <img src={customIconUrl} alt={t("Icona personalizzata di {0}", $language, [name])} class="size-full object-contain object-right" />
          {:else if steamAppId !== null}
            <SteamArtwork {steamAppId} asset="logo" caption={false} alt={t("Logo di {0}", $language, [name])} class="size-full object-contain object-right">
              {#snippet placeholder()}{@render nameText()}{/snippet}
            </SteamArtwork>
          {:else}
            {@render nameText()}
          {/if}
        </h2>
      </div>
    </section>

    <div class="relative z-10 flex flex-wrap items-end gap-2">
      {#if storeGame !== null}
        {#if job !== null}
          <DownloadProgressButton
            label={job.status === "queued" || job.status === "downloading" ? t("SCARICAMENTO", $language) : describeDownloadStatus(job.status).toUpperCase()}
            value={downloadProgress}
            progressLabel={t("Avanzamento download", $language)}
            onClick={() => selectSection("downloads")}
          />
        {:else if status?.availability === "verified" || status?.availability === "unverified"}
          <Button label={t("SCARICA", $language)} variant="download" size="hero" class="w-60" disabled={queueing} onClick={requestDownload}>
            <Icon name="download" size="size-6" />
          </Button>
        {/if}
      {:else if game !== null}
        <GameLaunchControls
          {game}
          {launch}
          variant="primary"
          size="hero"
          class="w-60"
          actionPending={$pendingGameId === game.id}
          cancelPending={$cancelPendingGameId === game.id}
          onPlay={playGame}
          onCancel={abortGameLaunch}
          onStop={stopGameProcess}
        />
      {/if}
      {#if versionOptions.length > 0}
        <GameVersionSelect {steamAppId} options={versionOptions} selectedId={selectedVersionId} onSelect={selectVersion} onAdd={game !== null && libraryVersions.length === 1 && steamAppId !== null ? () => { addVersionOpen = true; } : undefined} />
        {#if storeGame !== null && entry !== null}
          <DownloadMirrorSelect uris={mirrorUris} bind:value={selectedMirror} disabled={queueing || confirmOpen} />
        {/if}
      {/if}
      {#if game !== null}
        <Button label={t("Impostazioni del gioco", $language)} square variant="overlay" size="hero" onClick={() => (gameSettingsOpen = true)}>
          <Icon name="settings" size="size-6" />
        </Button>
      {/if}
    </div>

    {#if storeGame !== null && queueError !== null}
      <ErrorBanner message={queueError} />
    {/if}
    {#if storeGame !== null && $sourceRefreshError !== null}
      <ErrorBanner message={$sourceRefreshError} onRetry={() => void refreshSource()} retryLabel={t("Riprova", $language)} />
    {/if}
    {#if storeGame === null && artworkError !== null}
      <ErrorBanner message={artworkError} />
    {/if}


    {#if game !== null}<GameUpdateButton gameId={game.id} />{/if}

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
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry?.release.publishedAt == null ? t("non pubblicato", $language) : formatDate(entry.release.publishedAt)}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">{t("Dimensione", $language)}</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">{entry?.download.sizeBytes == null ? t("Dimensione sconosciuta", $language) : formatBytes(entry.download.sizeBytes)}</dd>
              </div>
              <div class="flex flex-wrap gap-x-3">
                <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Manifest</dt>
                <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                  {entry === null || $source.data.cachedAt === null ? t("non disponibile", $language) : formatDateTime($source.data.cachedAt)}
                  {#if entry !== null && $source.data.stale} <span class="text-legio-warning">{t("· cache scaduta", $language)}</span>{/if}
                </dd>
              </div>
            </dl>
            {#if storeGame !== null && $source.data.warning !== null}
              <p class="text-sm text-legio-warning">{$source.data.warning}</p>
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
    onRemoved={(removed) => {
      const nextId = nextVersionAfterRemoval(removed, $games.data, $playtime.data);
      gameSettingsOpen = false;
      if (nextId === null) closeGame();
      else openGame(nextId);
    }}
  />
{/if}

{#if artworkOpen && details !== null && steamAppId !== null}
  <ArtworkViewer
    {steamAppId}
    asset="hero"
    fallbackAsset="header"
    alt={t("Copertina di {0}", $language, [name])}
    onClose={() => (artworkOpen = false)}
  />
{/if}

{#if addVersionOpen && game !== null && steamAppId !== null}
  <AddGameDialog prefill={{ steamAppId, name: details?.name ?? game.name, suggestedName: `${game.name} (2)` }} onClose={() => (addVersionOpen = false)} />
{/if}

{#if storeGame !== null}
  <ConfirmDialog
    open={confirmOpen}
    title={t("Rilascio non verificato", $language)}
    message={t("Il rilascio di {0} non è classificato come verificato dalla fonte. Un archivio non verificato può contenere programmi dannosi. Procedere con il download e l'installazione?", $language, [name])}
    confirmLabel={t("Scarica comunque", $language)}
    tone="danger"
    confirmDisabled={queueing}
    onConfirm={() => { confirmOpen = false; if (confirmEntry !== null) void startDownload(confirmEntry.download.url, confirmEntry.release.version, true, confirmPreferredUri); }}
    onClose={() => (confirmOpen = false)}
  />
{/if}


{#snippet nameText()}
  <span class="block max-w-full truncate rounded-lg bg-zinc-950/60 px-3 py-1.5 text-right text-zinc-100 light:bg-zinc-100/70 light:text-zinc-900">{name}</span>
{/snippet}

{#snippet backdrop()}
  <div
    class="min-h-80 bg-gradient-to-br from-zinc-700 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  ></div>
{/snippet}
