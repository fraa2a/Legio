<script lang="ts">
  import { t, language } from "../../i18n";
  import type { Snippet } from "svelte";
  import { fade } from "svelte/transition";
  import {
    loadSteamImage,
    peekSteamImage,
    releaseSteamImage,
    retainSteamImage,
    type SteamAssetKind,
  } from "../../services/steam-details";
  import { steamDetails } from "../../stores/steam-details";
  import { toMessage } from "../../utils/errors";
  import { fadeDuration } from "../../utils/motion";

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
  const request = $derived({ steamAppId, asset, fallbackAsset, index, version: currentVersion, full });
  const cached = $derived(peekSteamImage(request));

  let url = $state<string | null>(null);
  let stale = $state(false);
  let warning = $state<string | null>(null);
  let error = $state<string | null>(null);
  let fromCache = $state(false);
  let loadedRequest = $state.raw<typeof request | null>(null);
  const displayedUrl = $derived(loadedRequest === request ? url : cached?.url ?? null);

  $effect(() => {
    const current = request;
    const cached = retainSteamImage(current);
    if (cached !== undefined) {
      loadedRequest = current;
      url = cached.url;
      stale = cached.stale;
      warning = cached.cacheWarning;
      error = null;
      fromCache = true;
      return () => releaseSteamImage(current);
    }

    loadedRequest = null;
    url = null;
    stale = false;
    warning = null;
    error = null;
    fromCache = false;

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
</script>

{#if displayedUrl !== null}
  <img src={displayedUrl} {alt} class={className} in:fade={{ duration: cached !== undefined || fromCache ? 0 : fadeDuration }} />
{:else if placeholder !== undefined}
  {@render placeholder()}
{:else if error !== null}
  <div class="flex items-center justify-center rounded-lg text-xs text-zinc-500 {className}">
    <span role="alert">{error}</span>
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
