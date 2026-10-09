<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import SteamArtwork from "../library/SteamArtwork.svelte";
  import { steamSummaryLine } from "../library/steam-description";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import { observeVisibility } from "../../utils/visibility";
  import { formatBytes } from "../../utils/format";
  import { availabilityMeta, type SourceStatus } from "./source-status";

  let {
    steamAppId,
    name,
    status,
    onOpen,
  }: {
    steamAppId: number;
    name: string;
    status: SourceStatus;
    onOpen: () => void;
  } = $props();

  let row: HTMLLIElement | undefined = $state();
  let visible = $state(false);
  const detailsState = $derived($steamDetails[steamAppId] ?? null);
  const entry = $derived(status.entry);
  const meta = $derived(availabilityMeta[status.availability]);
  const summary = $derived(steamSummaryLine(detailsState?.details?.shortDescription ?? null));

  $effect(() => {
    if (visible) return;
    return observeVisibility(row, () => (visible = true));
  });

  $effect(() => {
    void $language;
    if (visible) ensureSteamDetails(steamAppId);
  });
</script>

<li bind:this={row} class="tile group relative overflow-hidden rounded-xl bg-zinc-900 light:bg-white">
  <div
    class="legio-artwork-zoom absolute inset-0 bg-gradient-to-br from-zinc-800 to-zinc-950 light:from-zinc-200 light:to-zinc-100"
    aria-hidden="true"
  >
    {#if visible}
      <SteamArtwork
        {steamAppId}
        asset="hero_blur"
        version={detailsState?.cachedAt ?? null}
        caption={false}
        alt=""
        class="size-full scale-110 object-cover opacity-45 light:opacity-55"
      >
        {#snippet placeholder()}<span class="size-full"></span>{/snippet}
      </SteamArtwork>
    {/if}
  </div>
  <div
    class="absolute inset-0 bg-gradient-to-r from-zinc-950/40 via-zinc-950/70 to-zinc-950/90 light:from-white/60 light:via-white/85 light:to-white/95"
    aria-hidden="true"
  ></div>
  <button
    type="button"
    class="relative flex min-h-28 w-full items-stretch text-left transition-colors hover:bg-white/5 focus-visible:outline-2 focus-visible:outline-inset focus-visible:outline-white sm:min-h-32 light:hover:bg-zinc-900/5 light:focus-visible:outline-zinc-900"
    aria-label="{t("Dettagli di ", $language)}{name}{t(" nello Store", $language)}"
    onclick={onOpen}
  >
    <div class="relative w-56 shrink-0 overflow-hidden bg-zinc-800 sm:w-72 light:bg-zinc-200">
      {#if visible}
        <SteamArtwork
          {steamAppId}
          asset="header"
          version={detailsState?.cachedAt ?? null}
          caption={false}
          alt="{t("Copertina di ", $language)}{name}"
          class="absolute inset-0 size-full object-cover"
        >
          {#snippet placeholder()}
            <span class="absolute inset-0 flex size-full items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-900 text-3xl font-semibold text-zinc-400 light:from-zinc-300 light:to-zinc-100 light:text-zinc-500">{name.slice(0, 1)}</span>
          {/snippet}
        </SteamArtwork>
      {:else}
        <span class="absolute inset-0 flex size-full items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-900 text-3xl font-semibold text-zinc-400 light:from-zinc-300 light:to-zinc-100 light:text-zinc-500">{name.slice(0, 1)}</span>
      {/if}
    </div>
    <div class="flex min-w-0 flex-1 flex-wrap items-center justify-between gap-x-4 gap-y-1 py-3 pr-3 pl-4 sm:pr-4">
      <div class="flex min-w-0 flex-1 flex-col justify-center gap-1">
        <span class="truncate text-base font-semibold text-zinc-100 light:text-zinc-900">{name}</span>
        <span class="h-4 truncate text-xs leading-4 text-zinc-400 light:text-zinc-500">{summary ?? ""}</span>
      </div>
      <div class="flex shrink-0 flex-col items-end gap-1.5">
        {#if entry !== null}
          <span
            class="whitespace-nowrap text-xs tabular-nums text-zinc-400 light:text-zinc-500"
            title="{t("Versione ", $language)}{entry.release.version} · {formatBytes(entry.download.sizeBytes)}"
          >
            {entry.release.version} · {formatBytes(entry.download.sizeBytes)}
          </span>
        {/if}
        <Badge tone={meta.tone} title={t(meta.label, $language)} />
      </div>
    </div>
  </button>
</li>

<style>
  .tile {
    clip-path: inset(0 round 0.75rem);
  }
</style>
