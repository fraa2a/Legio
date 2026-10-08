<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy } from "svelte";
  import type { Game } from "../../services/local-state";
  import {
    acquireGameArtwork,
    gameArtworkRevision,
    resetGameBanner,
    resetGameIcon,
    setGameBanner,
    setGameIcon,
  } from "../../services/game-artwork";
  import { pickGameArtworkFile } from "../../services/dialog";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import { refreshSteamImages } from "../../services/steam-details";
  import { loadSteamDetails } from "../../stores/steam-details";
  import { showToast } from "../../stores/toast";

  let { game }: { game: Game } = $props();
  let iconUrl = $state<string | null>(null);
  let bannerUrl = $state<string | null>(null);
  let loading = $state(true);
  let pending = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  let destroyed = false;
  let requestId = 0;
  let releaseArtwork = () => {};
  const artworkGameId = $derived(game.id);
  onDestroy(() => { destroyed = true; requestId++; releaseArtwork(); });
  $effect(() => {
    const id = artworkGameId;
    void $gameArtworkRevision[`${id}:icon`];
    void $gameArtworkRevision[`${id}:banner`];
    void load(id);
  });

  async function load(id = artworkGameId): Promise<void> {
    const request = ++requestId;
    releaseArtwork();
    const icon = acquireGameArtwork(id, "icon");
    const banner = acquireGameArtwork(id, "banner");
    releaseArtwork = () => { icon.release(); banner.release(); };
    loading = true;
    error = null;
    try {
      const [iconResult, bannerResult] = await Promise.all([icon.ready, banner.ready]);
      if (destroyed || request !== requestId) return;
      iconUrl = iconResult;
      bannerUrl = bannerResult;
    } catch (cause) {
      if (!destroyed && request === requestId) error = toMessage(cause);
    } finally {
      if (!destroyed && request === requestId) loading = false;
    }
  }

  async function choose(kind: "icon" | "banner"): Promise<void> {
    try {
      const path = await pickGameArtworkFile();
      if (path === null || destroyed || pending) return;
      await runAction(async () => {
        if (kind === "icon") await setGameIcon(game.id, path);
        else await setGameBanner(game.id, path);
      });
    } catch (cause) {
      if (!destroyed) error = toMessage(cause);
    }
  }

  async function refreshImages(): Promise<void> {
    await runAction(async () => {
      if (game.steamAppId !== null) {
        await loadSteamDetails(game.steamAppId, true);
        await refreshSteamImages(game.steamAppId);
      }
      showToast(t("Immagini aggiornate.", $language), "success");
    });
  }

  async function reset(kind: "icon" | "banner"): Promise<void> {
    await runAction(async () => {
      if (kind === "icon") {
        await resetGameIcon(game.id);
      } else {
        await resetGameBanner(game.id);
      }
    });
  }

  async function runAction(action: () => Promise<void>): Promise<void> {
    if (destroyed || pending) return;
    pending = true;
    error = null;
    saved = false;
    try {
      await action();
      if (!destroyed) saved = true;
    } catch (cause) {
      if (!destroyed) error = toMessage(cause);
    } finally {
      if (!destroyed) pending = false;
    }
  }
</script>

<Panel title={t("Personalizzazione", $language)}>
  <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Scegli le immagini usate per questo gioco. I file vengono copiati nei dati di Legio.", $language)}</p>
  <div class="flex flex-wrap items-center gap-3">
    <Button label={t("Aggiorna immagini", $language)} variant="secondary" disabled={pending || game.steamAppId === null} onClick={() => void refreshImages()} />
    <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Riscarica le immagini Steam senza modificare quelle personalizzate.", $language)}</p>
  </div>
  {#if error !== null}<ErrorBanner message={error} onRetry={() => void load()} />{/if}
  {#if loading}
    <p class="text-sm text-zinc-400" role="status">{t("Caricamento delle immagini...", $language)}</p>
  {:else}
    <div class="grid gap-5">
      <section class="flex flex-col gap-3">
        <h3 class="text-base font-semibold">{t("Icona", $language)}</h3>
        {#if iconUrl !== null}<img src={iconUrl} alt="{t("Icona personalizzata di ", $language)}{game.name}" class="size-20 rounded-xl object-cover" />{/if}
        <div class="flex flex-wrap gap-2">
          <Button label={t("Scegli immagine", $language)} variant="secondary" disabled={pending} onClick={() => void choose("icon")} />
          {#if iconUrl !== null}<Button label={t("Ripristina", $language)} variant="secondary" disabled={pending} onClick={() => void reset("icon")} />{/if}
        </div>
      </section>
      <section class="flex flex-col gap-3">
        <h3 class="text-base font-semibold">Banner</h3>
        {#if bannerUrl !== null}<img src={bannerUrl} alt="{t("Banner personalizzato di ", $language)}{game.name}" class="aspect-[2.2/1] max-h-48 w-full rounded-xl object-cover" />{/if}
        <div class="flex flex-wrap gap-2">
          <Button label={t("Scegli immagine", $language)} variant="secondary" disabled={pending} onClick={() => void choose("banner")} />
          {#if bannerUrl !== null}<Button label={t("Ripristina", $language)} variant="secondary" disabled={pending} onClick={() => void reset("banner")} />{/if}
        </div>
      </section>
    </div>
  {/if}
  {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Immagine salvata.", $language)}</p>{/if}
</Panel>
