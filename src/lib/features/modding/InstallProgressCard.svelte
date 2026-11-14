<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import type { InstallProgress } from "../../services/modding";
  import { catalogDownload } from "../../stores/modding";
  import { formatBytes } from "../../utils/format";
  import { catalogPercent, displayPackageId, installPercent, installStageLabel } from "./modding-model";

  let {
    progress,
    cancelling = false,
    onCancel,
  }: {
    progress: InstallProgress | null;
    cancelling?: boolean;
    onCancel: () => void;
  } = $props();

  // Before the first install of a game the catalog is downloaded, with no install progress yet.
  const catalog = $derived(progress === null ? $catalogDownload : null);
  const catalogDone = $derived(catalog === null ? 0 : catalogPercent(catalog.chunksDone, catalog.chunksTotal));
  const title = $derived(progress !== null
    ? installStageLabel(progress, $language)
    : catalog === null ? t("Preparazione...", $language) : t("Aggiornamento del catalogo: {0}%", $language, [catalogDone]));
  const packageName = $derived(progress?.package == null ? null : displayPackageId(progress.package));
  const percent = $derived(progress === null ? catalogDone : installPercent(progress));
  const bytes = $derived(progress?.stage === "downloading" && progress.downloadedBytes !== null
    ? progress.totalBytes === null
      ? formatBytes(progress.downloadedBytes, $language)
      : `${formatBytes(progress.downloadedBytes, $language)} / ${formatBytes(progress.totalBytes, $language)}`
    : null);
</script>

<section class="flex flex-wrap items-center gap-4 rounded-2xl legio-glass p-4" aria-label={t("Installazione delle mod", $language)}>
  <Spinner />
  <div class="min-w-0 flex-1 basis-56" role="status" aria-live="polite">
    <p class="truncate text-sm font-medium text-legio-fg">
      {title}{#if packageName !== null}<span class="font-normal text-legio-fg-muted"> · {packageName}</span>{/if}
    </p>
    <div class="mt-0.5 flex flex-wrap gap-x-4 text-xs tabular-nums text-legio-fg-muted">
      {#if progress !== null && progress.total > 0}<span>{t("Pacchetto {0} di {1}", $language, [progress.current, progress.total])}</span>{/if}
      {#if bytes !== null}<span>{bytes}</span>{/if}
    </div>
    <ProgressBar value={percent} tone="accent" label={t("Avanzamento dell'installazione", $language)} class="mt-2 h-1.5" />
  </div>
  {#if cancelling || progress?.cancellable === true}
    <Button label={cancelling ? t("Annullamento...", $language) : t("Annulla", $language)} variant="danger-soft" size="sm" disabled={cancelling} onClick={onCancel} />
  {/if}
</section>
