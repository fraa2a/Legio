<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { Game } from "../../services/local-state";
  import {
    extractGameIcon,
    getGameBanner,
    getGameIcon,
    resetGameBanner,
    resetGameIcon,
    setGameBanner,
    setGameIcon,
    type GameArtwork,
  } from "../../services/game-artwork";
  import { pickGameArtworkFile } from "../../services/dialog";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";

  let { game }: { game: Game } = $props();
  let iconUrl = $state<string | null>(null);
  let bannerUrl = $state<string | null>(null);
  let loading = $state(true);
  let pending = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  onMount(() => void load());
  onDestroy(() => {
    if (iconUrl !== null) URL.revokeObjectURL(iconUrl);
    if (bannerUrl !== null) URL.revokeObjectURL(bannerUrl);
  });

  function imageUrl(artwork: GameArtwork | null): string | null {
    if (artwork === null) return null;
    return URL.createObjectURL(new Blob([new Uint8Array(artwork.bytes)], { type: artwork.contentType }));
  }

  function replaceUrl(current: string | null, artwork: GameArtwork | null): string | null {
    if (current !== null) URL.revokeObjectURL(current);
    return imageUrl(artwork);
  }

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      const [icon, banner] = await Promise.all([getGameIcon(game.id), getGameBanner(game.id)]);
      iconUrl = replaceUrl(iconUrl, icon);
      bannerUrl = replaceUrl(bannerUrl, banner);
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      loading = false;
    }
  }

  async function choose(kind: "icon" | "banner"): Promise<void> {
    try {
      const path = await pickGameArtworkFile();
      if (path === null) return;
      await runAction(async () => {
        const artwork = kind === "icon" ? await setGameIcon(game.id, path) : await setGameBanner(game.id, path);
        if (kind === "icon") iconUrl = replaceUrl(iconUrl, artwork);
        else bannerUrl = replaceUrl(bannerUrl, artwork);
      });
    } catch (cause) {
      error = toMessage(cause);
    }
  }

  async function extractIcon(): Promise<void> {
    await runAction(async () => {
      iconUrl = replaceUrl(iconUrl, await extractGameIcon(game.id));
    });
  }

  async function reset(kind: "icon" | "banner"): Promise<void> {
    await runAction(async () => {
      if (kind === "icon") {
        await resetGameIcon(game.id);
        iconUrl = replaceUrl(iconUrl, null);
      } else {
        await resetGameBanner(game.id);
        bannerUrl = replaceUrl(bannerUrl, null);
      }
    });
  }

  async function runAction(action: () => Promise<void>): Promise<void> {
    pending = true;
    error = null;
    saved = false;
    try {
      await action();
      saved = true;
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      pending = false;
    }
  }
</script>

<Panel title="Personalizzazione">
  <p class="text-sm text-zinc-400 light:text-zinc-600">Scegli le immagini usate per questo gioco. I file vengono copiati nei dati di Legio.</p>
  {#if error !== null}<ErrorBanner message={error} onRetry={() => void load()} />{/if}
  {#if loading}
    <p class="text-sm text-zinc-400" role="status">Caricamento delle immagini...</p>
  {:else}
    <div class="grid gap-5">
      <section class="flex flex-col gap-3">
        <h3 class="text-base font-semibold">Icona</h3>
        {#if iconUrl !== null}<img src={iconUrl} alt="Icona personalizzata di {game.name}" class="size-20 rounded-xl object-cover" />{/if}
        <div class="flex flex-wrap gap-2">
          <Button label="Scegli immagine" variant="secondary" disabled={pending} onClick={() => void choose("icon")} />
          {#if game.executablePath !== null}<Button label="Estrai dall'eseguibile" variant="secondary" disabled={pending} onClick={() => void extractIcon()} />{/if}
          {#if iconUrl !== null}<Button label="Ripristina" variant="secondary" disabled={pending} onClick={() => void reset("icon")} />{/if}
        </div>
      </section>
      <section class="flex flex-col gap-3">
        <h3 class="text-base font-semibold">Banner</h3>
        {#if bannerUrl !== null}<img src={bannerUrl} alt="Banner personalizzato di {game.name}" class="aspect-[2.2/1] max-h-48 w-full rounded-xl object-cover" />{/if}
        <div class="flex flex-wrap gap-2">
          <Button label="Scegli immagine" variant="secondary" disabled={pending} onClick={() => void choose("banner")} />
          {#if bannerUrl !== null}<Button label="Ripristina" variant="secondary" disabled={pending} onClick={() => void reset("banner")} />{/if}
        </div>
      </section>
    </div>
  {/if}
  {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Immagine salvata.</p>{/if}
</Panel>
