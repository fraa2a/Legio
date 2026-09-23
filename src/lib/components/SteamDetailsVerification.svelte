<script lang="ts">
  import { getSteamAsset, getSteamDetails, type SteamAssetKind, type SteamDetailsResult } from "../services/steam-details";

  type DisplayImage = { asset: SteamAssetKind; index?: number; label: string; url: string | null; stale: boolean; error: string | null; cacheWarning: string | null };

  let appIdInput = $state("");
  let result = $state<SteamDetailsResult | null>(null);
  let loading = $state<"cache" | "fetch" | null>(null);
  let error = $state<string | null>(null);
  let images = $state<DisplayImage[]>([]);
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

  function imageFailed(url: string) {
    URL.revokeObjectURL(url);
    images = images.map((image) => image.url === url ? { ...image, url: null, error: "Cached image could not be displayed" } : image);
  }

  $effect(() => {
    const current = result?.details;
    if (!current) {
      images = [];
      return;
    }

    const requests: Pick<DisplayImage, "asset" | "index" | "label">[] = [];
    if (current.assets.header) requests.push({ asset: "header", label: "Header" });
    else if (current.assets.capsule) requests.push({ asset: "capsule", label: "Capsule" });
    current.assets.screenshots.slice(0, 3).forEach((screenshot, index) => {
      if (screenshot.thumbnail || screenshot.full) requests.push({ asset: "screenshot", index, label: `Screenshot ${index + 1}` });
    });
    images = requests.map((request) => ({ ...request, url: null, stale: false, error: null, cacheWarning: null }));

    let active = true;
    const objectUrls: string[] = [];
    requests.forEach((request, position) => {
      void getSteamAsset(current.steamAppId, request.asset, request.index).then(
        (response) => {
          if (!active) return;
          const url = URL.createObjectURL(new Blob([new Uint8Array(response.bytes)], { type: response.contentType }));
          objectUrls.push(url);
          images = images.map((image, index) => index === position ? { ...image, url, stale: response.stale, cacheWarning: response.cacheWarning } : image);
        },
        (reason) => {
          if (active) images = images.map((image, index) => index === position ? { ...image, error: messageFor(reason) } : image);
        },
      );
    });
    return () => {
      active = false;
      objectUrls.forEach((url) => URL.revokeObjectURL(url));
    };
  });
</script>

<section class="mt-8" aria-labelledby="steam-details-heading">
  <h2 id="steam-details-heading" class="text-xl font-semibold">Steam details and assets</h2>
  <p class="mt-2 text-sm text-slate-400">Enter a Steam App ID to read local details without a metadata network request. Fetch explicitly requests fresh details from Steam and saves them locally. No Steam login or API key is needed.</p>
  <p class="mt-2 text-sm text-slate-400">The details cache holds up to 128 entries and is fresh for 24 hours. Images load through Legio's local cache. An uncached image may require a network connection.</p>
  <label class="mt-4 block text-sm" for="steam-details-id">Steam App ID</label>
  <input id="steam-details-id" class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2 text-slate-100" type="text" inputmode="numeric" autocomplete="off" placeholder="Example: 570" value={appIdInput} oninput={changeId} aria-describedby="steam-details-id-help" />
  <p id="steam-details-id-help" class="mt-2 text-xs text-slate-400">{appIdInput && appId === null ? "Enter a whole number from 1 to 4294967295." : "Changing the ID reads cached details. Metadata fetches require the button; displayed images may load automatically."}</p>
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
        {#if images.length === 0}
          <p class="mt-4 text-sm text-slate-400">No image metadata is available for this game.</p>
        {:else}
          <div class="mt-4 grid gap-4 sm:grid-cols-2">
            {#each images as image (`${image.asset}-${image.index ?? 0}`)}
              <figure class="rounded border border-slate-800 p-3">
                {#if image.url}
                  <img class="h-auto max-w-full rounded" src={image.url} alt={`${details.name} ${image.label.toLowerCase()}`} onerror={(event) => imageFailed((event.currentTarget as HTMLImageElement).src)} />
                  {#if image.stale}<figcaption class="mt-2 text-xs text-amber-300">Cached {image.label.toLowerCase()} may be stale.</figcaption>{/if}
                  {#if image.cacheWarning}<p class="mt-2 break-words text-xs text-amber-300" role="alert">{image.cacheWarning}</p>{/if}
                {:else if image.error}
                  <p class="break-words text-sm" role="alert">{image.label} unavailable: {image.error}. Read the local details cache to retry.</p>
                {:else}
                  <p class="text-sm" role="status">Loading {image.label.toLowerCase()}...</p>
                {/if}
              </figure>
            {/each}
          </div>
        {/if}
      </article>
    {:else}
      <p class="mt-3 rounded border border-dashed border-slate-600 p-4 text-sm" role="status">No cached details for Steam App ID {appId}. Fetch details from Steam to populate the cache.</p>
    {/if}
  {:else if appIdInput === ""}
    <p class="mt-3 text-sm" role="status">Enter an App ID to inspect its cached details.</p>
  {/if}
</section>
