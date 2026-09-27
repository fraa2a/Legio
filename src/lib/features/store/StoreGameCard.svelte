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

<ArtworkTile {steamAppId} {monogram}>
  <button
    type="button"
    class="flex flex-1 flex-col text-left"
    aria-label="Dettagli di {name} nello store"
    onclick={onOpen}
  >
    <div class="absolute right-2 top-2">
      <Badge tone={meta.tone} title={meta.label} />
    </div>

    <div
      class="relative mt-auto flex flex-col gap-1 bg-gradient-to-t from-zinc-950/90 via-zinc-950/50 to-transparent px-4 pt-8 pb-3 light:from-zinc-100/90 light:via-zinc-100/50"
    >
      {#if entry !== null}
        <span class="truncate text-xs text-zinc-400 light:text-zinc-600">
          v{entry.release.version} · {formatBytes(entry.download.sizeBytes)}
        </span>
      {/if}
      <span class="truncate font-medium text-zinc-50 light:text-zinc-900">{name}</span>
    </div>
  </button>
</ArtworkTile>
