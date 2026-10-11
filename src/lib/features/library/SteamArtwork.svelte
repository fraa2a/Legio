<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Snippet } from "svelte";
  import { fade } from "svelte/transition";
  import {
    acquireSteamImage,
    steamImageRevision,
    peekSteamImage,
    retainSteamImage,
    type RetainedSteamImage,
    type SteamAssetKind,
  } from "../../services/steam-details";
  import { toMessage } from "../../utils/errors";
  import { fadeDuration } from "../../utils/motion";
  import { artworkDisplayWidth } from "../../stores/artwork-display";
  import { isArtworkCancelled, type ArtworkWork } from "../../services/artwork-work";
  import { logApplicationEvent } from "../../services/application-log";

  let {
    steamAppId,
    asset,
    fallbackAsset = null,
    index = null,
    screenshotIdentity = undefined,
    origin = undefined,
    full = false,
    alt = "",
    caption = true,
    class: className = "",
    placeholder,
  }: {
    steamAppId: number;
    asset: SteamAssetKind;
    fallbackAsset?: SteamAssetKind | null;
    index?: number | null;
    version?: number | null;
    screenshotIdentity?: string;
    origin?: ArtworkWork["origin"];
    full?: boolean;
    alt?: string;
    caption?: boolean;
    class?: string;
    placeholder?: Snippet;
  } = $props();

  const request = $derived({ steamAppId, asset, fallbackAsset, index, version: null, full, screenshotIdentity, displayWidth: $artworkDisplayWidth });
  const cached = $derived.by(() => {
    void $steamImageRevision;
    return peekSteamImage(request);
  });

  let publishedImage = $state.raw<RetainedSteamImage | null>(null);
  let url = $state<string | null>(null);
  let stale = $state(false);
  let warning = $state<string | null>(null);
  let error = $state<string | null>(null);
  let fromCache = $state(false);
  let loadedRequest = $state.raw<typeof request | null>(null);
  let failedUrl = $state<string | null>(null);
  const candidateUrl = $derived(publishedImage?.url ?? (loadedRequest === request ? url : cached?.url ?? null));
  const visibleStale = $derived(publishedImage?.stale ?? stale);
  const visibleWarning = $derived(publishedImage ? publishedImage.cacheWarning : warning);
  const displayedUrl = $derived(candidateUrl === failedUrl ? null : candidateUrl);

  function imageFailed(event: Event): void {
    failedUrl = (event.currentTarget as HTMLImageElement).src;
    error = "Artwork display failed";
    logApplicationEvent("warn", "console", `Artwork display failed; asset: ${asset}`);
  }

  $effect(() => {
    const current = request;
    const consumer = acquireSteamImage(current, origin ?? (asset === "hero_blur" ? "decorative" : asset === "screenshot" ? "prefetch" : "visible"));
    let cancelled = false;
    loadedRequest = current;
    url = consumer.image?.url ?? null;
    stale = consumer.image?.stale ?? false;
    warning = consumer.image?.cacheWarning ?? null;
    error = null;
    fromCache = consumer.image !== undefined;
    void consumer.promise.then(image => {
      if (cancelled) return;
      url = image.url; stale = image.stale; warning = image.cacheWarning;
    }, failure => { if (!cancelled && !isArtworkCancelled(failure)) error = toMessage(failure); });
    return () => { cancelled = true; consumer.release(); };
  });
  $effect(() => {
    void $steamImageRevision;
    const retained = retainSteamImage(request);
    publishedImage = retained ?? null;
    return () => retained?.release();
  });
</script>

{#if displayedUrl !== null}
  <img draggable="false" src={displayedUrl} {alt} class={className} onerror={imageFailed} in:fade={{ duration: cached !== undefined || fromCache ? 0 : fadeDuration }} />
{:else if placeholder !== undefined}
  {@render placeholder()}
{:else if error !== null || (candidateUrl !== null && candidateUrl === failedUrl)}
  <div class="flex items-center justify-center rounded-lg text-xs text-zinc-500 {className}">
    <span aria-hidden="true">{alt.trim().charAt(0).toUpperCase() || "?"}</span>
  </div>
{:else}
  <div class="flex items-center justify-center rounded-lg {className}">
    <span role="status" class="sr-only">{t("Caricamento immagine...", $language)}</span>
  </div>
{/if}

{#if caption && (visibleStale || visibleWarning !== null)}
  <p class="mt-1 text-xs text-amber-300 light:text-amber-800">
    {visibleWarning ?? t("Immagine servita dalla cache locale.", $language)}
  </p>
{/if}
