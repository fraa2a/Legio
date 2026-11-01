<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
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

<SettingsGroup icon="mods" title={t("Mod", $language)}>
  {#if loadError !== null}
    <ErrorBanner message={loadError} onRetry={() => void loadUsage()} retryLabel={t("Riprova", $language)} />
  {:else}
    {#each rows as row (row.kind)}
      <SettingsRow label="{row.title} · {formatBytes(row.bytes, $language)}" description={row.description}>
        <Button label={t("Elimina", $language)} variant="secondary" size="sm" disabled={row.bytes === 0} onClick={() => ask(row)} />
      </SettingsRow>
    {:else}
      {#each [0, 1, 2] as placeholder (placeholder)}<Skeleton class="h-10" />{/each}
    {/each}
  {/if}
</SettingsGroup>

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
