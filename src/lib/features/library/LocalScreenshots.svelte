<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import {
    importGameScreenshotBackground,
    listGameScreenshots,
    loadGameScreenshotUrl,
    openGameScreenshotsFolder,
    setGameScreenshotsFolder,
    type GameScreenshotPage,
  } from "../../services/game-screenshots";
  import { cleanupBackgrounds } from "../../services/appearance";
  import { pickGameDirectory } from "../../services/dialog";
  import { settings, updateSettings } from "../../stores/settings";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Pagination from "../../components/ui/Pagination.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import ScreenshotGallery from "./ScreenshotGallery.svelte";
  import { pageCountOf, pageOffset, runLimited, toLocalImages } from "./local-screenshots";

  let { game }: { game: Game } = $props();

  let result = $state.raw<GameScreenshotPage | null>(null);
  let page = $state(1);
  let thumbs = $state.raw<Record<string, string>>({});
  let fulls = $state.raw<Record<string, string>>({});
  let active = $state(0);
  let loading = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let destroyed = false;
  let request = 0;

  const entries = $derived(result?.entries ?? []);
  const images = $derived(toLocalImages(entries, thumbs, fulls));
  const activeEntry = $derived(entries[active] ?? null);
  const pages = $derived(result === null ? 1 : pageCountOf(result.total, result.pageSize));

  onMount(() => {
    void load(1);
    return () => { destroyed = true; request++; revokeAll(); };
  });

  function revokeAll(): void {
    for (const url of [...Object.values(thumbs), ...Object.values(fulls)]) URL.revokeObjectURL(url);
    thumbs = {};
    fulls = {};
  }

  async function load(nextPage: number): Promise<void> {
    const current = ++request;
    loading = true;
    error = null;
    try {
      const size = result?.pageSize ?? 12;
      const loaded = await listGameScreenshots(game.id, pageOffset(nextPage, size));
      if (destroyed || current !== request) return;
      revokeAll();
      result = loaded;
      page = nextPage;
      active = 0;
      loading = false;
      await runLimited(loaded.entries, 3, async (entry) => {
        const url = await loadGameScreenshotUrl(game.id, entry.id, true);
        if (destroyed || current !== request) { URL.revokeObjectURL(url); return; }
        thumbs = { ...thumbs, [entry.id]: url };
      });
    } catch (cause) {
      if (!destroyed && current === request) error = toMessage(cause);
    } finally {
      if (!destroyed && current === request) loading = false;
    }
  }

  // The full image loads only for the screenshot on display.
  async function showFull(index: number): Promise<void> {
    active = index;
    const entry = entries[index];
    if (entry === undefined || fulls[entry.id] !== undefined) return;
    const current = request;
    try {
      const url = await loadGameScreenshotUrl(game.id, entry.id, false);
      if (destroyed || current !== request) { URL.revokeObjectURL(url); return; }
      fulls = { ...fulls, [entry.id]: url };
    } catch (cause) {
      if (!destroyed && current === request) error = toMessage(cause);
    }
  }

  async function run(action: () => Promise<void>): Promise<void> {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await action();
    } catch (cause) {
      if (!destroyed) error = toMessage(cause);
    } finally {
      if (!destroyed) busy = false;
    }
  }

  const openFolder = () => run(() => openGameScreenshotsFolder(game.id, activeEntry?.id ?? null));

  const chooseFolder = () => run(async () => {
    const folder = await pickGameDirectory(result?.folder, t("Seleziona la cartella degli screenshot", $language));
    if (folder === null || destroyed) return;
    await setGameScreenshotsFolder(game.id, folder);
    await load(1);
  });

  const resetFolder = () => run(async () => {
    await setGameScreenshotsFolder(game.id, null);
    await load(1);
  });

  const setWallpaper = () => run(async () => {
    if (activeEntry === null) return;
    const id = await importGameScreenshotBackground(game.id, activeEntry.id);
    await updateSettings({ appearance: { ...$settings.data.appearance, background: id } });
    await cleanupBackgrounds();
    if (!destroyed) showToast(t("Sfondo aggiornato.", $language), "success");
  });
</script>

<Panel title={t("Screenshot locali", $language)}>
  {#snippet actions()}
    <div class="flex flex-wrap gap-2">
      <Button label={t("Apri cartella", $language)} variant="secondary" size="sm" disabled={busy} onClick={() => void openFolder()}><Icon name="folder-open" /></Button>
      <Button label={t("Cambia cartella", $language)} variant="secondary" size="sm" disabled={busy} onClick={() => void chooseFolder()} />
      {#if result?.customFolder}<Button label={t("Cartella predefinita", $language)} variant="ghost" size="sm" disabled={busy} onClick={() => void resetFolder()} />{/if}
    </div>
  {/snippet}
  {#if error !== null}<ErrorBanner message={error} onRetry={() => void load(page)} />{/if}
  {#if loading}
    <div role="status" aria-label={t("Caricamento...", $language)} class="flex flex-col gap-3">
      <Skeleton class="aspect-video w-full" />
      <div class="flex gap-2.5"><Skeleton class="aspect-video w-28" /><Skeleton class="aspect-video w-28" /><Skeleton class="aspect-video w-28" /></div>
    </div>
  {:else if result !== null && entries.length === 0}
    <EmptyState icon="image" title={t("Nessuno screenshot", $language)} description={t("Gli screenshot di Steam e quelli salvati in {0} compaiono qui.", $language, [result.folder])} />
  {:else if result !== null}
    <ScreenshotGallery name={game.name} local={images} onActive={(index) => void showFull(index)} />
    <div class="flex flex-wrap items-center justify-between gap-3">
      <p class="text-xs text-legio-fg-muted">{activeEntry?.name ?? ""}</p>
      <Button label={t("Imposta come sfondo", $language)} variant="secondary" size="sm" disabled={busy || activeEntry === null} onClick={() => void setWallpaper()} />
    </div>
    {#if pages > 1}<Pagination {page} pageCount={pages} disabled={busy} onChange={(next) => void load(next)} />{/if}
  {/if}
</Panel>
