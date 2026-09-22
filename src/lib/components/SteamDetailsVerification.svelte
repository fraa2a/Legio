<script lang="ts">
  import { getSteamDetails, type SteamDetailsResult } from "../services/steam-details";

  let appIdInput = $state("");
  let result = $state<SteamDetailsResult | null>(null);
  let loading = $state<"cache" | "fetch" | null>(null);
  let error = $state<string | null>(null);
  let generation = 0;
  const appId = $derived(/^\d+$/.test(appIdInput) && Number(appIdInput) > 0 && Number(appIdInput) <= 4294967295 ? Number(appIdInput) : null);
  const details = $derived(result?.details);

  function messageFor(reason: unknown): string {
    if (typeof reason === "object" && reason !== null && "message" in reason && typeof reason.message === "string") {
      const kind = "kind" in reason && typeof reason.kind === "string" ? `${reason.kind}: ` : "";
      const status = "status" in reason && typeof reason.status === "number" ? ` (HTTP ${reason.status})` : "";
      return `${kind}${reason.message}${status}`;
    }
    return String(reason);
  }

  async function load(refresh: boolean) {
    if (appId === null) return;
    const request = ++generation;
    loading = refresh ? "fetch" : "cache";
    error = null;
    try {
      const response = await getSteamDetails(appId, refresh);
      if (request === generation) result = response;
    } catch (reason) {
      if (request === generation) error = `${refresh ? "Steam fetch" : "Local cache read"} failed: ${messageFor(reason)}`;
    } finally {
      if (request === generation) loading = null;
    }
  }

  function changeId(event: Event) {
    ++generation;
    appIdInput = (event.currentTarget as HTMLInputElement).value;
    result = null;
    error = null;
    loading = null;
    void load(false);
  }
</script>

<section class="mt-8" aria-labelledby="steam-details-heading">
  <h2 id="steam-details-heading" class="text-xl font-semibold">Steam details and assets</h2>
  <p class="mt-2 text-sm text-slate-400">Enter a Steam App ID to read local details without a metadata network request. Fetch explicitly requests fresh details from Steam and saves them locally. No Steam login or API key is needed.</p>
  <p class="mt-2 text-sm text-slate-400">The cache holds up to 128 entries, at most 64 KiB each, fresh for 24 hours. Only metadata and image URLs are cached, not image bytes. Displayed images load online from Steam and may be unavailable offline.</p>
  <label class="mt-4 block text-sm" for="steam-details-id">Steam App ID</label>
  <input id="steam-details-id" class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2 text-slate-100" type="text" inputmode="numeric" autocomplete="off" placeholder="Example: 570" value={appIdInput} oninput={changeId} aria-describedby="steam-details-id-help" />
  <p id="steam-details-id-help" class="mt-2 text-xs text-slate-400">{appIdInput && appId === null ? "Enter a whole number from 1 to 4294967295." : "Changing the ID reads only its local cache. Fetching is always explicit."}</p>
  <div class="mt-4 flex flex-wrap gap-3">
    <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={appId === null || loading !== null} onclick={() => load(true)}>{loading === "fetch" ? "Fetching..." : "Fetch details from Steam"}</button>
    <button class="text-sm underline disabled:opacity-50" type="button" disabled={appId === null || loading !== null} onclick={() => load(false)}>Read local details cache</button>
  </div>
  {#if loading}
    <p class="mt-3 text-sm" role="status">{loading === "fetch" ? "Fetching details from Steam..." : "Reading local Steam details..."}</p>
  {/if}
  {#if error}
    <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">{error}. {details ? "Previously loaded cached details remain below." : "Run the native app and retry."}</p>
  {/if}
  {#if result}
    {#if result.cachedAt !== null}
      <p class="mt-3 text-xs text-slate-400">Cache timestamp: {new Date(result.cachedAt * 1000).toLocaleString()}. {result.stale ? "Cache is stale. Fetch details to update it." : "Cached details are fresh."}</p>
    {/if}
    {#if details}
      <article class="mt-3 rounded border border-slate-800 p-4">
        <h3 class="break-words text-lg font-semibold">{details.name}</h3>
        <p class="mt-1 text-xs text-slate-400">Steam App ID {details.steamAppId} | Type: {details.appType}</p>
        {#if details.shortDescription}<p class="mt-3 whitespace-pre-line break-words text-sm">{details.shortDescription}</p>{/if}
        <dl class="mt-3 space-y-2 text-sm">
          {#if details.developers.length}<div><dt class="text-slate-400">Developers</dt><dd>{details.developers.join(", ")}</dd></div>{/if}
          {#if details.publishers.length}<div><dt class="text-slate-400">Publishers</dt><dd>{details.publishers.join(", ")}</dd></div>{/if}
          {#if details.genres.length}<div><dt class="text-slate-400">Genres</dt><dd>{details.genres.join(", ")}</dd></div>{/if}
          {#if details.platforms}<div><dt class="text-slate-400">Platforms reported by Steam</dt><dd>{[details.platforms.windows ? "Windows" : null, details.platforms.mac ? "macOS" : null, details.platforms.linux ? "Linux" : null].filter(Boolean).join(", ") || "None reported"}</dd></div>{/if}
          {#if details.releaseDate}<div><dt class="text-slate-400">Release</dt><dd>{details.releaseDate.comingSoon ? "Coming soon. " : ""}{details.releaseDate.date}</dd></div>{/if}
        </dl>
        {#if details.assets.header}
          <img class="mt-4 h-auto max-w-full rounded" src={details.assets.header} alt={`${details.name} header`} loading="lazy" referrerpolicy="no-referrer" />
        {:else if details.assets.capsule}
          <img class="mt-4 h-auto max-w-full rounded" src={details.assets.capsule} alt={`${details.name} capsule`} loading="lazy" referrerpolicy="no-referrer" />
        {/if}
        {#each details.assets.screenshots.slice(0, 3) as screenshot, index (screenshot)}
          {#if screenshot.thumbnail || screenshot.full}
            <img class="mt-3 h-auto max-w-full rounded" src={screenshot.thumbnail ?? screenshot.full ?? ""} alt={`${details.name} screenshot ${index + 1}`} loading="lazy" referrerpolicy="no-referrer" />
          {/if}
        {/each}
      </article>
    {:else}
      <p class="mt-3 rounded border border-dashed border-slate-600 p-4 text-sm" role="status">No cached details for Steam App ID {appId}. Fetch details from Steam to populate the cache.</p>
    {/if}
  {:else if appIdInput === ""}
    <p class="mt-3 text-sm" role="status">Enter an App ID to inspect its cached details.</p>
  {/if}
</section>
