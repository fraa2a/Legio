<script lang="ts">
  import { getSteamAsset, type SteamAssetKind } from "../../services/steam-details";
  import { toMessage } from "../../utils/errors";

  let {
    steamAppId,
    asset,
    index = null,
    alt = "",
    refreshKey = 0,
    class: className = "",
  }: {
    steamAppId: number;
    asset: SteamAssetKind;
    index?: number | null;
    alt?: string;
    refreshKey?: number;
    class?: string;
  } = $props();

  let url = $state<string | null>(null);
  let stale = $state(false);
  let warning = $state<string | null>(null);
  let error = $state<string | null>(null);

  const request = $derived({ steamAppId, asset, index, refreshKey });

  $effect(() => {
    const { steamAppId: appId, asset: kind, index: shot } = request;

    url = null;
    stale = false;
    warning = null;
    error = null;

    let objectUrl: string | null = null;
    let cancelled = false;
    void getSteamAsset(appId, kind, shot ?? undefined).then(
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
  <img src={url} {alt} class={className} />
{:else}
  <div class="flex items-center justify-center rounded-lg bg-white/5 text-xs text-zinc-500 {className}">
    {#if error !== null}
      <span role="alert">{error}</span>
    {:else}
      <span role="status">Caricamento immagine...</span>
    {/if}
  </div>
{/if}

{#if stale || warning !== null}
  <p class="mt-1 text-xs text-amber-300 light:text-amber-800">
    {warning ?? "Immagine servita dalla cache locale."}
  </p>
{/if}
