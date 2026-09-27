<script lang="ts">
  import { describeDownloadStatus } from "../../services/downloads";
  import { toMessage } from "../../utils/errors";
  import { formatBytes, formatDate, formatDateTime } from "../../utils/format";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import { downloads, queueJob } from "../../stores/downloads";
  import { selectSection } from "../../stores/navigation";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import { refreshSource, source, sourceRefreshError } from "../../stores/source";
  import ArtworkViewer from "../library/ArtworkViewer.svelte";
  import SteamArtwork from "../library/SteamArtwork.svelte";
  import SteamMetadata from "../library/SteamMetadata.svelte";
  import SystemRequirements from "../library/SystemRequirements.svelte";
  import { availabilityMeta, sourceStatusFor } from "./source-status";

  let {
    steamAppId,
    name,
  }: {
    steamAppId: number;
    name: string;
  } = $props();

  const detailsState = $derived($steamDetails[steamAppId] ?? null);
  const details = $derived(detailsState?.details ?? null);
  const status = $derived(sourceStatusFor($source.data.manifest, steamAppId));
  const meta = $derived(availabilityMeta[status.availability]);
  const entry = $derived(status.entry);
  const job = $derived(
    $downloads.data.find((candidate) => candidate.steamAppId === steamAppId) ?? null,
  );
  const cachedAt = $derived($source.data.cachedAt);
  const sourceWarning = $derived($source.data.warning);

  const headline = $derived.by(() => {
    if (details === null) return null;
    const date = details.releaseDate;
    const release = date === null ? null : date.comingSoon ? "In arrivo" : date.date;
    return [details.appType, release].filter((part) => part !== null).join(" · ");
  });

  let confirmOpen = $state(false);
  let artworkOpen = $state(false);
  let queueing = $state(false);
  let queueError = $state<string | null>(null);

  $effect(() => {
    ensureSteamDetails(steamAppId);
  });

  async function startDownload(acceptUnverified: boolean): Promise<void> {
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
    if (status.availability === "unverified") {
      confirmOpen = true;
      return;
    }
    void startDownload(false);
  }
</script>

<div class="flex min-h-full flex-col gap-4">
  <section class="relative isolate aspect-[3.1/1] w-full min-w-0 overflow-hidden rounded-2xl bg-zinc-800 light:bg-zinc-200">
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
    {#if details !== null}
      <button type="button" class="absolute inset-0 z-0 cursor-zoom-in" aria-label="Ingrandisci copertina" onclick={() => (artworkOpen = true)}></button>
    {/if}
    <div class="pointer-events-none absolute inset-0 z-[1] bg-gradient-to-t from-zinc-950/95 via-zinc-950/35 to-transparent"></div>
    <div class="absolute inset-x-0 bottom-0 z-10 flex flex-wrap items-end justify-between gap-4 p-5 sm:p-7">
      <div class="pointer-events-none flex min-w-0 flex-col items-start gap-2">
        <Badge tone={meta.tone} title={meta.label} />
        <h2 class="max-w-full text-2xl font-semibold text-white sm:text-3xl">
          <SteamArtwork {steamAppId} asset="logo" version={detailsState?.cachedAt ?? null} caption={false} alt="Logo di {name}" class="max-h-20 max-w-80 object-contain object-left">
            {#snippet placeholder()}<span class="truncate">{name}</span>{/snippet}
          </SteamArtwork>
        </h2>
        {#if headline !== null}<p class="text-sm text-zinc-200">{headline}</p>{/if}
        {#if job !== null}
          <p class="text-sm text-zinc-300">{describeDownloadStatus(job.status)} · versione {job.releaseVersion}</p>
        {/if}
      </div>
      <div class="flex flex-wrap gap-2">
        {#if job !== null}
          <Button label="Vai ai download" variant="secondary" onClick={() => selectSection("downloads")} />
        {:else if status.availability === "verified" || status.availability === "unverified"}
          <Button label="Scarica" disabled={queueing} onClick={requestDownload} />
        {/if}
      </div>
    </div>
  </section>

  {#if queueError !== null}
    <ErrorBanner message={queueError} />
  {/if}
  {#if $sourceRefreshError !== null}
    <ErrorBanner message={$sourceRefreshError} onRetry={() => void refreshSource()} retryLabel="Riprova" />
  {/if}

  <div class="grid flex-1 gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
    <div class="flex min-w-0 flex-col gap-3">
      <SteamMetadata {steamAppId} />
    </div>
    <div class="flex min-w-0 flex-col gap-3">
      <SystemRequirements
        requirements={details?.systemRequirements ?? null}
        loading={detailsState?.status === "loading"}
      />
      <Panel title="Info gioco">
        <div class="flex flex-col gap-3">
          <p class="text-sm text-zinc-300 light:text-zinc-700">{meta.description}</p>
          <dl class="grid gap-2 text-sm">
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Versione</dt>
              <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                {entry?.release.version ?? "non pubblicata"}
              </dd>
            </div>
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Pubblicato</dt>
              <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                {entry === null ? "non pubblicato" : formatDate(entry.release.publishedAt)}
              </dd>
            </div>
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Dimensione</dt>
              <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                {entry === null ? "non disponibile" : formatBytes(entry.download.sizeBytes)}
              </dd>
            </div>
            <div class="flex flex-wrap gap-x-3">
              <dt class="w-40 shrink-0 text-zinc-400 light:text-zinc-600">Manifest</dt>
              <dd class="min-w-0 flex-1 break-words text-zinc-100 light:text-zinc-900">
                {cachedAt === null ? "non disponibile" : formatDateTime(cachedAt)}
                {#if $source.data.stale}
                  <span class="text-amber-300 light:text-amber-800"> · cache scaduta</span>
                {/if}
              </dd>
            </div>
          </dl>
          {#if sourceWarning !== null}
            <p class="text-sm text-amber-300 light:text-amber-800">{sourceWarning}</p>
          {/if}
        </div>
      </Panel>
    </div>
  </div>
</div>

{#if artworkOpen && details !== null}
  <ArtworkViewer {steamAppId} asset="hero" fallbackAsset="header" alt="Copertina di {name}" onClose={() => (artworkOpen = false)} />
{/if}

<Dialog open={confirmOpen} title="Rilascio non verificato" onClose={() => (confirmOpen = false)}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Il rilascio di {name} non è stato verificato dallo staff Legio. Un archivio non verificato può contenere
    programmi dannosi. Procedere con il download e l'installazione?
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Annulla" variant="secondary" onClick={() => (confirmOpen = false)} />
    <Button
      label="Scarica comunque"
      variant="danger"
      disabled={queueing}
      onClick={() => {
        confirmOpen = false;
        void startDownload(true);
      }}
    />
  </div>
</Dialog>

{#snippet backdrop()}
  <div
    class="absolute inset-0 bg-gradient-to-br from-zinc-700 to-zinc-900 light:from-zinc-300 light:to-zinc-100"
    aria-hidden="true"
  ></div>
{/snippet}
