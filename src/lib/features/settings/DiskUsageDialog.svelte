<script lang="ts">
  import { onMount } from "svelte";
  import { language, t } from "../../i18n";
  import { cancelDiskUsage, openGameFolder } from "../../services/disk-usage";
  import type { Game } from "../../services/local-state";
  import { diskUsage, requestDiskUsage, watchDiskUsage } from "../../stores/disk-usage";
  import { gameLibraries } from "../../stores/game-libraries";
  import { games } from "../../stores/games";
  import { playtime } from "../../stores/playtime";
  import { toMessage } from "../../utils/errors";
  import { formatBytes, formatDate } from "../../utils/format";
  import Badge from "../../components/ui/Badge.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import DownloadCover from "../downloads/DownloadCover.svelte";
  import UninstallGameDialog from "../library/UninstallGameDialog.svelte";
  import { buildSizeRows, totalBytes } from "./disk-usage-model";

  let { onClose }: { onClose: () => void } = $props();

  let uninstalling = $state<Game | null>(null);
  let error = $state<string | null>(null);
  const rows = $derived(buildSizeRows($games.data, $diskUsage.data.games, $gameLibraries.data, $playtime.data));
  const loading = $derived($games.status === "idle" || $games.status === "loading" && $games.data.length === 0);
  const computing = $derived($diskUsage.data.computing);

  onMount(() => {
    void Promise.all([games.load(), gameLibraries.load(), playtime.load()]);
    const stop = watchDiskUsage(false);
    requestDiskUsage().catch((cause: unknown) => { error = toMessage(cause); });
    return () => {
      stop();
      void cancelDiskUsage();
    };
  });

  async function openFolder(game: Game): Promise<void> {
    error = null;
    try { await openGameFolder(game.id); }
    catch (cause) { error = toMessage(cause); }
  }

  function libraryLabel(library: string | null, game: Game): string {
    if (library !== null) return library;
    return game.steamInstallPath !== null ? "Steam" : t("Esterna", $language);
  }
</script>

<Dialog open title={t("Cosa occupa spazio", $language)} size="wide" {onClose}>
  <div class="flex flex-col gap-4">
    <p class="text-sm text-legio-fg-muted">
      {#if computing}
        {t("Calcolo delle dimensioni in corso...", $language)}
      {:else}
        {t("Spazio occupato dai giochi installati: {0}.", $language, [formatBytes(totalBytes(rows), $language)])}
      {/if}
    </p>
    {#if error !== null}<ErrorBanner message={error} />{/if}
    {#if $diskUsage.error !== null}<ErrorBanner message={$diskUsage.error} onRetry={() => void diskUsage.reload()} />{/if}
    {#if loading}
      <div class="flex flex-col gap-2" role="status" aria-label={t("Caricamento...", $language)}>
        {#each [0, 1, 2] as key (key)}<Skeleton class="h-16 w-full" />{/each}
      </div>
    {:else if rows.length === 0}
      <EmptyState icon="library" title={t("Nessun gioco installato", $language)} description={t("Quando installi un gioco, qui vedrai quanto spazio occupa.", $language)} />
    {:else}
      <ul class="flex max-h-[60vh] flex-col gap-2 overflow-y-auto" aria-label={t("Giochi per dimensione", $language)}>
        {#each rows as row (row.game.id)}
          <li class="flex items-center gap-3 rounded-card bg-legio-fill p-3">
            <DownloadCover steamAppId={row.game.steamAppId} name={row.game.name} class="aspect-[2/3] w-9" radius="control" />
            <div class="flex min-w-0 flex-1 flex-col gap-0.5">
              <p class="truncate text-sm font-medium text-legio-fg">{row.game.name}</p>
              <p class="truncate text-xs text-legio-fg-muted" title={libraryLabel(row.library, row.game)}>{libraryLabel(row.library, row.game)}</p>
              <p class="text-xs text-legio-fg-subtle">{row.lastPlayedAt === null ? t("Mai giocato", $language) : t("Ultima partita: {0}", $language, [formatDate(row.lastPlayedAt, $language)])}</p>
            </div>
            <div class="flex shrink-0 flex-col items-end gap-1">
              {#if row.bytes !== null}
                <span class="text-sm font-semibold text-legio-fg">{formatBytes(row.bytes, $language)}</span>
                {#if row.partial}<Badge tone="warning" title={t("Parziale", $language)} />{/if}
              {:else if computing}
                <Skeleton class="h-5 w-16" />
              {:else}
                <span class="text-sm text-legio-fg-subtle">{t("Non calcolata", $language)}</span>
              {/if}
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <IconButton label={t("Apri cartella", $language)} title={t("Apri cartella", $language)} onClick={() => void openFolder(row.game)}><Icon name="folder" /></IconButton>
              {#if row.removable}
                <IconButton label={t("Disinstalla", $language)} title={t("Disinstalla", $language)} variant="danger" onClick={() => (uninstalling = row.game)}><Icon name="trash" /></IconButton>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Dialog>

{#if uninstalling !== null}
  <UninstallGameDialog game={uninstalling} onClose={() => (uninstalling = null)} onRemoved={() => (uninstalling = null)} />
{/if}
