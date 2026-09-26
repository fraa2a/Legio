<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade } from "svelte/transition";
  import { getSteamAsset, type SteamAssetKind } from "../../services/steam-details";
  import { toMessage } from "../../utils/errors";
  import { fadeDuration } from "../../utils/motion";

  let {
    steamAppId,
    asset,
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
    index?: number | null;
    version?: number | null;
    full?: boolean;
    alt?: string;
    caption?: boolean;
    class?: string;
    placeholder?: Snippet;
  } = $props();

  let url = $state<string | null>(null);
  let stale = $state(false);
  let warning = $state<string | null>(null);
  let error = $state<string | null>(null);

  const request = $derived({ steamAppId, asset, index, version, full });

  $effect(() => {
    const { steamAppId: appId, asset: kind, index: shot, full: large } = request;

    url = null;
    stale = false;
    warning = null;
    error = null;

    let objectUrl: string | null = null;
    let cancelled = false;
    void getSteamAsset(appId, kind, shot ?? undefined, large).then(
      (result) => {
        if (cancelled) return;
        objectUrl = URL.createObjectURL(
          new Blob([Uint8Array.from(result.bytes)], { type: result.contentType }),
        );
        url = objectUrl;
        stale = result.stale;
        warning = result.cacheWarning;
      },
      (failure: unknown) => {
        if (!cancelled) error = toMessage(failure);
      },
    );

    return () => {
      cancelled = true;
      if (objectUrl !== null) URL.revokeObjectURL(objectUrl);
    };
  });
</script>

{#if url !== null}
  <img src={url} {alt} class={className} in:fade={{ duration: fadeDuration }} />
{:else if placeholder !== undefined}
  {@render placeholder()}
{:else if error !== null}
  <div class="flex items-center justify-center rounded-lg text-xs text-zinc-500 {className}">
    <span role="alert">{error}</span>
  </div>
{:else}
  <div class="flex items-center justify-center rounded-lg {className}">
    <span role="status" class="sr-only">Caricamento immagine...</span>
  </div>
{/if}

{#if caption && (stale || warning !== null)}
  <p class="mt-1 text-xs text-amber-300 light:text-amber-800">
    {warning ?? "Immagine servita dalla cache locale."}
  </p>
{/if}
