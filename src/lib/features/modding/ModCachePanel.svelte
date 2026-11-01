<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import { clearModCache, getModCacheUsage, type ModCacheUsage } from "../../services/modding";
  import { resetCatalogs } from "../../stores/modding";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { formatBytes } from "../../utils/format";
  import { cacheRows, clearedMessage, describeToolsError, type CacheRow } from "./mod-tools-model";

  let usage = $state<ModCacheUsage | null>(null);
  let loadError = $state<string | null>(null);
  let target = $state<CacheRow | null>(null);
  let clearing = $state(false);
  let clearError = $state<string | null>(null);

  const rows = $derived(usage === null ? [] : cacheRows(usage, $language));

  async function loadUsage(): Promise<void> {
    loadError = null;
    try {
      usage = await getModCacheUsage();
    } catch (error) {
      loadError = describeToolsError(toMessage(error), $language);
    }
  }

  onMount(() => void loadUsage());

  function ask(row: CacheRow): void {
    clearError = null;
    target = row;
  }

  async function confirm(): Promise<void> {
    const row = target;
    if (row === null || clearing) return;
    clearing = true;
    clearError = null;
    try {
      await clearModCache(row.kind);
      if (row.kind === "catalog") resetCatalogs();
      target = null;
      showToast(clearedMessage(row.kind, $language), "success");
      await loadUsage();
    } catch (error) {
      clearError = describeToolsError(toMessage(error), $language);
    } finally {
      clearing = false;
    }
  }
</script>

<Panel title={t("Spazio su disco", $language)}>
  {#if loadError !== null}
    <ErrorBanner message={loadError} onRetry={() => void loadUsage()} retryLabel={t("Riprova", $language)} />
  {:else}
    <ul class="flex flex-col gap-2">
      {#each rows as row (row.kind)}
        <li class="flex flex-wrap items-center gap-3 rounded-xl legio-glass p-3">
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-zinc-100 light:text-zinc-900">{row.title} <span class="text-zinc-400 light:text-zinc-600">&middot; {formatBytes(row.bytes, $language)}</span></p>
            <p class="text-xs text-zinc-500">{row.description}</p>
          </div>
          <Button label={t("Elimina", $language)} variant="secondary" size="sm" disabled={row.bytes === 0} onClick={() => ask(row)} />
        </li>
      {:else}
        {#each [0, 1, 2] as placeholder (placeholder)}<li><Skeleton class="h-16" /></li>{/each}
      {/each}
    </ul>
  {/if}
</Panel>

<ConfirmDialog
  open={target !== null}
  title={target?.title ?? ""}
  message={target?.confirmMessage}
  confirmLabel={clearing ? t("Eliminazione...", $language) : t("Elimina", $language)}
  tone="danger"
  pending={clearing}
  error={clearError}
  onConfirm={() => void confirm()}
  onClose={() => (target = null)}
/>
