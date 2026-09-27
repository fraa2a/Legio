<script lang="ts">
  import ArtworkTile from "../../components/ui/ArtworkTile.svelte";
  import Badge from "../../components/ui/Badge.svelte";
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

  const entry = $derived(status.entry);
  const meta = $derived(availabilityMeta[status.availability]);
  const monogram = $derived(name.trim().charAt(0).toUpperCase() || "?");
</script>

<ArtworkTile {steamAppId} {monogram} class="ring-1 ring-white/10 transition-shadow hover:ring-white/35 light:ring-zinc-900/10 light:hover:ring-zinc-900/30">
  <button
    type="button"
    class="flex flex-1 flex-col text-left focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-white"
    aria-label="Dettagli di {name} nello store"
    onclick={onOpen}
  >
    <div class="absolute right-2 top-2">
      <Badge tone={meta.tone} title={meta.label} />
    </div>

    <div
      class="relative mt-auto flex flex-col gap-1 bg-gradient-to-t from-zinc-950/95 via-zinc-950/65 to-transparent px-4 pt-9 pb-4 light:from-zinc-100/95 light:via-zinc-100/65"
    >
      {#if entry !== null}
        <span class="truncate text-xs text-zinc-400 light:text-zinc-600">
          v{entry.release.version} · {formatBytes(entry.download.sizeBytes)}
        </span>
      {/if}
      <span class="truncate text-base font-semibold text-zinc-50 light:text-zinc-900">{name}</span>
      <span class="text-xs text-zinc-300 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 light:text-zinc-700">Apri dettagli</span>
    </div>
  </button>
</ArtworkTile>
