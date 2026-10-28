<script lang="ts">
  import { t, language } from "../../i18n";
  import type { LoadStatus } from "../../stores/resource";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import { formatBytes } from "../../utils/format";

  let {
    folder,
    status,
    error,
    downloadBytes,
    speedBps,
    bandwidthLimit,
    onRetry,
    onOpenFolder,
  }: {
    folder: { directory: string; freeBytes: number; totalBytes: number } | null;
    status: LoadStatus;
    error: string | null;
    downloadBytes: number;
    speedBps: number;
    bandwidthLimit: number;
    onRetry: () => void;
    onOpenFolder: () => void;
  } = $props();

  const usedPercent = $derived.by(() => {
    if (folder === null || folder.totalBytes <= 0) return 0;
    return Math.min(100, ((folder.totalBytes - folder.freeBytes) / folder.totalBytes) * 100);
  });
  const insufficient = $derived(folder !== null && downloadBytes > folder.freeBytes);
</script>

<Panel title={t("Sistema", $language)}>
  <div class="flex flex-col gap-4">
    {#if status === "idle" || status === "loading"}
      <div class="flex items-center gap-2.5 text-sm text-zinc-400 light:text-zinc-600" role="status">
        <Spinner size="sm" />{t("Lettura dello spazio su disco...", $language)}</div>
    {:else if status === "error" && error !== null}
      <ErrorBanner message={error} onRetry={onRetry} />
    {/if}

    {#if folder !== null}
      <div class="flex flex-col gap-1.5">
        <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Spazio disponibile", $language)}</p>
        <p class="text-sm font-medium tabular-nums text-zinc-50 light:text-zinc-900">
          {formatBytes(folder.freeBytes, $language)}
          <span class="font-normal text-zinc-500">{t("su {0}", $language, [formatBytes(folder.totalBytes, $language)])}</span>
        </p>
        <ProgressBar
          value={usedPercent}
          tone={insufficient ? "danger" : "accent"}
          label={t("Spazio occupato sul disco di installazione", $language)}
          class="h-1.5"
        />
      </div>
    {/if}

    <div class="flex flex-col gap-1.5">
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Dimensione download", $language)}</p>
      <p
        class="text-sm font-medium tabular-nums {insufficient ? 'text-legio-error' : 'text-zinc-50 light:text-zinc-900'}"
      >
        {formatBytes(downloadBytes, $language)}
        {#if insufficient}
          <span class="font-normal">{t("· dimensione maggiore dello spazio disponibile", $language)}</span>
        {/if}
      </p>
    </div>

    <p class="text-xs text-zinc-500">{t("Lo spazio di installazione è ancora sconosciuto e include estrazione e file temporanei.", $language)}</p>
    {#if folder !== null}
      <div class="flex flex-col gap-1.5">
        <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Directory di installazione", $language)}</p>
        <p class="break-all text-xs text-zinc-300 light:text-zinc-700">{folder.directory}</p>
        <Button label={t("Apri la cartella", $language)} variant="secondary" class="mt-1 self-start" onClick={onOpenFolder} />
      </div>
    {/if}

    <div class="flex flex-col gap-1.5 border-t border-white/10 pt-4 light:border-zinc-900/10">
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Utilizzo della rete", $language)}</p>
      <p class="text-sm font-medium tabular-nums text-zinc-50 light:text-zinc-900">
        {formatBytes(speedBps, $language)}/s
      </p>
      <p class="text-xs text-zinc-500 light:text-zinc-500">
        {bandwidthLimit > 0 ? t("Limite impostato a {0}/s", $language, [formatBytes(bandwidthLimit, $language)]) : t("Nessun limite di banda", $language)}
      </p>
    </div>
  </div>
</Panel>
