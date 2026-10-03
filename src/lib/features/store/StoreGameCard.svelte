<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import SteamArtwork from "../library/SteamArtwork.svelte";
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

  $effect(() => {
    if (visible) return;
    return observeVisibility(row, () => (visible = true));
  });

  $effect(() => {
    void $language;
    if (visible) ensureSteamDetails(steamAppId);
  });
</script>

<li bind:this={row} class="overflow-hidden rounded-xl border border-white/10 bg-zinc-900 light:border-zinc-900/10 light:bg-white">
  <button
    type="button"
    class="flex min-h-24 w-full items-center gap-3 p-2 text-left transition-colors hover:bg-white/5 focus-visible:outline-2 focus-visible:outline-inset focus-visible:outline-white light:hover:bg-zinc-900/5 light:focus-visible:outline-zinc-900"
    aria-label="{t("Dettagli di ", $language)}{name}{t(" nello Store", $language)}"
    onclick={onOpen}
  >
    <div class="h-20 w-28 shrink-0 overflow-hidden rounded-lg bg-zinc-800 sm:h-24 sm:w-44 light:bg-zinc-200">
      {#if visible}
        <SteamArtwork
          {steamAppId}
          asset="header"
          version={detailsState?.cachedAt ?? null}
          caption={false}
          alt="{t("Copertina di ", $language)}{name}"
          class="size-full object-cover"
        >
          {#snippet placeholder()}
            <span class="flex size-full items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-900 text-3xl font-semibold text-zinc-400 light:from-zinc-300 light:to-zinc-100 light:text-zinc-500">{name.slice(0, 1)}</span>
          {/snippet}
        </SteamArtwork>
      {:else}
        <span class="flex size-full items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-900 text-3xl font-semibold text-zinc-400 light:from-zinc-300 light:to-zinc-100 light:text-zinc-500">{name.slice(0, 1)}</span>
      {/if}
    </div>
    <div class="flex min-w-0 flex-1 flex-wrap items-center justify-between gap-x-4 gap-y-1 px-1 sm:pr-3">
      <div class="flex min-w-0 flex-1 flex-col justify-center gap-1">
        <span class="truncate text-base font-semibold text-zinc-100 light:text-zinc-900">{name}</span>
        {#if entry !== null}
          <span class="text-xs text-zinc-400 light:text-zinc-500">{t("Versione ", $language)}{entry.release.version} · {formatBytes(entry.download.sizeBytes)}</span>
        {/if}
      </div>
      <Badge tone={meta.tone} title={t(meta.label, $language)} />
    </div>
  </button>
</li>
