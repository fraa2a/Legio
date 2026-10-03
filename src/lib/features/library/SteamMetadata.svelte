<script lang="ts">
  import { t, language } from "../../i18n";
  import { loadSteamDetails, steamDetails } from "../../stores/steam-details";
  import Badge from "../../components/ui/Badge.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import ScreenshotGallery from "./ScreenshotGallery.svelte";
  import { renderSteamDescription } from "./steam-description";

  let { steamAppId }: { steamAppId: number } = $props();

  const detailsState = $derived($steamDetails[steamAppId] ?? null);
  const details = $derived(detailsState?.details ?? null);
  const cachedAt = $derived(detailsState?.cachedAt ?? null);
  const screenshots = $derived(details?.assets.screenshots.slice(0, 6) ?? []);
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

  {#if detailsState !== null && detailsState.status === "loading"}
    <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">{t("Caricamento dettagli...", $language)}</p>
  {/if}

  {#if detailsState !== null && detailsState.status === "error" && detailsState.error !== null}
    <ErrorBanner message={detailsState.error} onRetry={refresh} />
  {/if}

  {#if details === null}
    <p class="text-sm text-zinc-500">{t("\n      Nessun dato Steam disponibile per questo titolo.\n    ", $language)}</p>
  {:else}
    {#if screenshots.length > 0}
      <ScreenshotGallery {steamAppId} name={details.name} version={cachedAt} {screenshots} />
    {/if}

    {#if description !== null}
      <div use:renderSteamDescription={description} class="space-y-3 text-sm leading-relaxed text-zinc-200 [&_a]:text-sky-300 [&_a]:underline [&_blockquote]:border-l-2 [&_blockquote]:border-white/20 [&_blockquote]:pl-4 [&_h1]:text-xl [&_h1]:font-semibold [&_h1]:text-white [&_h2]:mt-5 [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:text-white [&_h3]:font-semibold [&_h3]:text-white [&_hr]:border-white/10 [&_li]:ml-5 [&_ol]:list-decimal [&_p]:my-2 [&_pre]:overflow-x-auto [&_ul]:list-disc light:text-zinc-800 light:[&_a]:text-sky-700 light:[&_h1]:text-zinc-900 light:[&_h2]:text-zinc-900 light:[&_h3]:text-zinc-900"></div>
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
          <dt class="w-24 shrink-0 text-zinc-400 light:text-zinc-600">{t("Sviluppatori", $language)}</dt>
          <dd class="min-w-0 flex-1 text-zinc-100 light:text-zinc-900">
            {details.developers.join(", ")}
          </dd>
        </div>
      {/if}
      {#if details.publishers.length > 0}
        <div class="flex flex-wrap gap-x-3">
          <dt class="w-24 shrink-0 text-zinc-400 light:text-zinc-600">{t("Editori", $language)}</dt>
          <dd class="min-w-0 flex-1 text-zinc-100 light:text-zinc-900">
            {details.publishers.join(", ")}
          </dd>
        </div>
      {/if}
    </dl>

  {/if}
</Panel>
