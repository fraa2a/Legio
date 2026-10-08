<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Snippet } from "svelte";
  import { fade } from "svelte/transition";
  import {
    loadSteamImage,
    steamImageRevision,
    peekSteamImage,
    releaseSteamImage,
    retainSteamImage,
    type SteamAssetKind,
  } from "../../services/steam-details";
  import { steamDetails } from "../../stores/steam-details";
  import { windowActive } from "../../stores/window-activity";
  import { toMessage } from "../../utils/errors";
  import { fadeDuration } from "../../utils/motion";
  import { artworkDisplayWidth } from "../../stores/artwork-display";

  let {
    steamAppId,
    asset,
    fallbackAsset = null,
    index = null,
    version = null,
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
    full?: boolean;
    alt?: string;
    caption?: boolean;
    class?: string;
    placeholder?: Snippet;
  } = $props();

  const currentVersion = $derived(version ?? $steamDetails[steamAppId]?.cachedAt ?? null);
  const request = $derived({ steamAppId, asset, fallbackAsset, index, version: currentVersion, full, displayWidth: $artworkDisplayWidth });
  const cached = $derived.by(() => {
    void $steamImageRevision;
    return peekSteamImage(request);
  });

  let url = $state<string | null>(null);
  let stale = $state(false);
  let warning = $state<string | null>(null);
  let error = $state<string | null>(null);
  let fromCache = $state(false);
  let refreshAt = $state<number | null>(null);
  let loadedRequest = $state.raw<typeof request | null>(null);
  const displayedUrl = $derived(loadedRequest === request ? url : cached?.url ?? null);

  $effect(() => {
    void $steamImageRevision;
    const current = request;
    const cached = retainSteamImage(current);
    if (cached !== undefined) {
      loadedRequest = current;
      url = cached.url;
      stale = cached.stale;
      warning = cached.cacheWarning;
      error = null;
      fromCache = true;
      refreshAt = cached.refreshAt;
      return () => releaseSteamImage(current);
    }

    loadedRequest = null;
    url = null;
    stale = false;
    warning = null;
    error = null;
    fromCache = false;
    refreshAt = null;

    let cancelled = false;
    let retained = false;
    void loadSteamImage(current).then(
      () => {
        if (cancelled) return;
        const image = retainSteamImage(current);
        if (image === undefined) return;
        retained = true;
        loadedRequest = current;
        url = image.url;
        stale = image.stale;
        warning = image.cacheWarning;
        refreshAt = image.refreshAt;
      },
      (failure: unknown) => {
        if (!cancelled) error = toMessage(failure);
      },
    );

    return () => {
      cancelled = true;
      if (retained) releaseSteamImage(current);
    };
  });
  $effect(() => {
    const current = request;
    if (refreshAt === null || !$windowActive) return;
    const timer = setTimeout(() => { void loadSteamImage(current); }, Math.max(0, refreshAt - Date.now()));
    return () => clearTimeout(timer);
  });
</script>

{#if displayedUrl !== null}
  <img draggable="false" src={displayedUrl} {alt} class={className} in:fade={{ duration: cached !== undefined || fromCache ? 0 : fadeDuration }} />
{:else if placeholder !== undefined}
  {@render placeholder()}
{:else if error !== null}
  <div class="flex items-center justify-center rounded-lg text-xs text-zinc-500 {className}">
    <span aria-hidden="true">{alt.trim().charAt(0).toUpperCase() || "?"}</span>
  </div>
{:else}
  <div class="flex items-center justify-center rounded-lg {className}">
    <span role="status" class="sr-only">{t("Caricamento immagine...", $language)}</span>
  </div>
{/if}

{#if caption && (stale || warning !== null)}
  <p class="mt-1 text-xs text-amber-300 light:text-amber-800">
    {warning ?? t("Immagine servita dalla cache locale.", $language)}
  </p>
{/if}
