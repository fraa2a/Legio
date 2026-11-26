<script lang="ts">
  import { onMount } from "svelte";
  import { language, t } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import { diskUsage, requestDiskUsage, watchDiskUsage } from "../../stores/disk-usage";
  import { formatBytes } from "../../utils/format";

  let { game }: { game: Game } = $props();

  let requestedFor: string | null = null;
  const hasFolder = $derived(game.steamInstallPath !== null || game.installationRoot !== null);
  const size = $derived($diskUsage.data.games.find(entry => entry.gameId === game.id));

  onMount(() => watchDiskUsage(false));

  $effect(() => {
    if (!hasFolder || size !== undefined || $diskUsage.status !== "ready" || $diskUsage.data.computing || requestedFor === game.id) return;
    requestedFor = game.id;
    // A failure leaves the line hidden; the size dialog reports it.
    void requestDiskUsage(game.id).catch(() => {});
  });
</script>

{#if size !== undefined}
  <p class="text-sm text-legio-fg-muted">
    {t("Spazio su disco: {0}", $language, [formatBytes(size.bytes, $language)])}{size.partial ? ` (${t("parziale", $language)})` : ""}
  </p>
{/if}
