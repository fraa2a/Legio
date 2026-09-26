<script lang="ts">
  import { loadSteamDetails, steamDetails } from "../../stores/steam-details";
  import Button from "../../components/ui/Button.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";

  let { steamAppId }: { steamAppId: number } = $props();

  let refreshKey = $state(0);

  const details = $derived($steamDetails.appId === steamAppId ? $steamDetails.details : null);
  const detailsState = $derived($steamDetails.appId === steamAppId ? $steamDetails : null);
  const screenshots = $derived(details?.assets.screenshots.slice(0, 3) ?? []);

  async function refresh(): Promise<void> {
    refreshKey += 1;
    await loadSteamDetails(steamAppId, true);
  }
</script>

<section class="flex flex-col gap-3">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <h3 class="text-sm font-semibold text-zinc-200 light:text-zinc-800">Dettagli Steam</h3>
    <div class="flex items-center gap-2">
      {#if detailsState?.stale}
        <Badge tone="warning" title="Cache scaduta" />
      {/if}
      <Button label="Aggiorna" variant="secondary" onClick={() => void refresh()} />
    </div>
  </div>

  {#if detailsState !== null && detailsState.status === "loading"}
    <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">Caricamento dettagli...</p>
  {/if}

  {#if detailsState !== null && detailsState.status === "error" && detailsState.error !== null}
    <ErrorBanner
      message={detailsState.error}
      onRetry={() => void loadSteamDetails(steamAppId, true)}
    />
  {/if}

  {#if details === null}
    <p class="text-sm text-zinc-500">
      Nessun dato Steam disponibile per questo titolo.
    </p>
  {:else}
    <SteamArtwork
      {steamAppId}
      asset="header"
      {refreshKey}
      alt=""
      class="h-28 w-full rounded-lg object-cover"
    />

    <div class="flex flex-col gap-1">
      <p class="font-medium text-zinc-50 light:text-zinc-900">{details.name}</p>
      <p class="text-xs text-zinc-500">
        {details.appType}
        {#if details.releaseDate !== null}
          · {details.releaseDate.comingSoon ? "In arrivo" : details.releaseDate.date}
        {/if}
        {#if details.platforms !== null}
          · {[
            details.platforms.windows ? "Windows" : null,
            details.platforms.mac ? "macOS" : null,
            details.platforms.linux ? "Linux" : null,
          ]
            .filter((entry) => entry !== null)
            .join(", ")}
        {/if}
      </p>
    </div>

    {#if details.shortDescription !== null}
      <p class="text-sm text-zinc-300 light:text-zinc-700">{details.shortDescription}</p>
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

    <dl class="grid gap-1 text-xs text-zinc-400 light:text-zinc-600">
      {#if details.developers.length > 0}
        <div class="flex gap-2">
          <dt class="w-24 shrink-0">Sviluppatori</dt>
          <dd class="min-w-0 break-words">{details.developers.join(", ")}</dd>
        </div>
      {/if}
      {#if details.publishers.length > 0}
        <div class="flex gap-2">
          <dt class="w-24 shrink-0">Editori</dt>
          <dd class="min-w-0 break-words">{details.publishers.join(", ")}</dd>
        </div>
      {/if}
    </dl>

    {#if screenshots.length > 0}
      <ul class="grid gap-2 sm:grid-cols-3">
        {#each screenshots as screenshot, position (screenshot.thumbnail ?? screenshot.full)}
          <li>
            <SteamArtwork
              {steamAppId}
              asset="screenshot"
              index={position}
              {refreshKey}
              alt=""
              class="aspect-video w-full rounded-lg object-cover"
            />
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>
