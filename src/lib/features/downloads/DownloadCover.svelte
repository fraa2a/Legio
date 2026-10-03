<script lang="ts">
  import { language } from "../../i18n";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import SteamArtwork from "../library/SteamArtwork.svelte";

  let {
    steamAppId,
    name,
    class: className = "",
  }: {
    steamAppId: number | null;
    name: string;
    class?: string;
  } = $props();

  const monogram = $derived(name.trim().charAt(0).toUpperCase() || "?");
  const detailsState = $derived(steamAppId === null ? null : ($steamDetails[steamAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);

  $effect(() => {
    void $language; if (steamAppId !== null) ensureSteamDetails(steamAppId);
  });
</script>

<div
  class="flex shrink-0 items-center justify-center overflow-hidden rounded-xl bg-gradient-to-br from-zinc-700 to-zinc-950 light:from-zinc-300 light:to-zinc-100 {className}"
  aria-hidden="true"
>
  {#if steamAppId !== null && details !== null}
    <SteamArtwork
      {steamAppId}
      asset="library_capsule"
      fallbackAsset="header"
      version={detailsState?.cachedAt ?? null}
      caption={false}
      class="size-full object-cover"
    />
  {:else}
    <span class="text-2xl font-semibold text-zinc-400/90 light:text-zinc-500">{monogram}</span>
  {/if}
</div>
