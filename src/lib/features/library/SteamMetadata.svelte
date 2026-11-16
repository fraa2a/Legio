<script lang="ts">
  import { t, language } from "../../i18n";
  import { loadSteamDetails, steamDetails } from "../../stores/steam-details";
  import Badge from "../../components/ui/Badge.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import ScreenshotGallery from "./ScreenshotGallery.svelte";
  import { renderSteamDescription } from "./steam-description";

  let { steamAppId }: { steamAppId: number } = $props();

  const detailsState = $derived($steamDetails[steamAppId] ?? null);
  const details = $derived(detailsState?.details ?? null);
  const screenshots = $derived(details?.assets.screenshots.slice(0, 6) ?? []);
  const loading = $derived(detailsState?.status === "loading" && details === null);
  const description = $derived(details?.detailedDescription ?? details?.shortDescription ?? null);

  function refresh(): void {
    void loadSteamDetails(steamAppId, true).catch(() => undefined);
  }
</script>

<Panel title={t("Dettagli Steam", $language)} class="flex-1">
  {#snippet actions()}
    <div class="flex items-center gap-2">
      {#if detailsState?.stale}
        <Badge tone="warning" title={t("Cache scaduta", $language)} />
      {/if}
    </div>
  {/snippet}

  {#if loading}
    <div class="flex flex-col gap-rhythm-row" role="status" aria-busy="true">
      <span class="sr-only">{t("Caricamento dettagli...", $language)}</span>
      <Skeleton class="aspect-video w-full" />
      <div class="flex flex-col gap-2">
        <Skeleton class="h-4 w-full" />
        <Skeleton class="h-4 w-11/12" />
        <Skeleton class="h-4 w-2/3" />
      </div>
    </div>
  {/if}

  {#if detailsState !== null && detailsState.status === "error" && detailsState.error !== null}
    <ErrorBanner message={detailsState.error} onRetry={refresh} />
  {/if}

  {#if details === null}
    {#if !loading}<p class="text-sm text-legio-fg-subtle">{t("Nessun dato Steam disponibile per questo titolo.", $language)}</p>{/if}
  {:else}
    {#if screenshots.length > 0 || (details.assets.movies?.length ?? 0) > 0}
      <ScreenshotGallery {steamAppId} name={details.name} {screenshots} movies={details.assets.movies ?? []} />
    {/if}

    {#if description !== null}
      <div use:renderSteamDescription={description} class="space-y-3 text-sm leading-relaxed text-legio-fg [&_blockquote]:border-l-2 [&_blockquote]:border-legio-line [&_blockquote]:pl-4 [&_h1]:text-xl [&_h1]:font-semibold [&_h1]:text-legio-fg [&_h2]:mt-5 [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:text-legio-fg [&_h3]:font-semibold [&_h3]:text-legio-fg [&_hr]:border-legio-line-subtle [&_li]:ml-5 [&_ol]:list-decimal [&_p]:my-2 [&_pre]:overflow-x-auto [&_ul]:list-disc"></div>
    {/if}

    {#if details.genres.length > 0}
      <ul class="flex flex-wrap gap-2">
        {#each details.genres as genre (genre)}
          <li>
            <Badge tone="neutral" title={genre} />
          </li>
        {/each}
      </ul>
    {/if}

    <dl class="grid gap-3 text-sm">
      {#if details.developers.length > 0}
        <div class="flex flex-wrap gap-x-3">
          <dt class="w-24 shrink-0 text-legio-fg-muted">{t("Sviluppatori", $language)}</dt>
          <dd class="min-w-0 flex-1 text-legio-fg">
            {details.developers.join(", ")}
          </dd>
        </div>
      {/if}
      {#if details.publishers.length > 0}
        <div class="flex flex-wrap gap-x-3">
          <dt class="w-24 shrink-0 text-legio-fg-muted">{t("Editori", $language)}</dt>
          <dd class="min-w-0 flex-1 text-legio-fg">
            {details.publishers.join(", ")}
          </dd>
        </div>
      {/if}
    </dl>

  {/if}
</Panel>
