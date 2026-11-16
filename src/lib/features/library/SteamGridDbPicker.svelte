<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import type { Game } from "../../services/local-state";
  import {
    applySteamGridDbArtwork,
    hasSteamGridDbKey,
    listSteamGridDbArtwork,
    searchSteamGridDbGames,
    type SteamGridDbGame,
    type SteamGridDbImage,
  } from "../../services/steamgriddb";
  import { toMessage } from "../../utils/errors";
  import { showToast } from "../../stores/toast";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import EmptyState from "../../components/ui/EmptyState.svelte";
  import SegmentedControl from "../../components/ui/SegmentedControl.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Skeleton from "../../components/ui/Skeleton.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { pickerView, steamGridDbTabs } from "./steamgriddb";

  let { game, onClose }: { game: Game; onClose: () => void } = $props();

  let hasKey = $state<boolean | null>(null);
  // The search box starts from the game name and is then edited freely.
  // svelte-ignore state_referenced_locally
  let query = $state(game.name);
  let games = $state.raw<SteamGridDbGame[]>([]);
  let sgdbGameId = $state<string | null>(null);
  let tab = $state<(typeof steamGridDbTabs)[number]["id"]>("hero");
  let images = $state.raw<SteamGridDbImage[]>([]);
  let loading = $state(false);
  let searched = $state(false);
  let applying = $state(false);
  let error = $state<string | null>(null);
  let destroyed = false;
  let request = 0;

  const activeTab = $derived(steamGridDbTabs.find((candidate) => candidate.id === tab) ?? steamGridDbTabs[0]);
  const view = $derived(pickerView({ hasKey, loading, error, games: games.length, images: images.length, searched }));

  onMount(() => {
    void start();
    return () => { destroyed = true; request++; };
  });

  async function start(): Promise<void> {
    try {
      hasKey = await hasSteamGridDbKey();
    } catch (cause) {
      hasKey = true;
      error = toMessage(cause);
      return;
    }
    if (hasKey && !destroyed) await search();
  }

  async function search(): Promise<void> {
    const current = ++request;
    loading = true;
    error = null;
    try {
      const found = await searchSteamGridDbGames(query, game.steamAppId);
      if (destroyed || current !== request) return;
      games = found;
      searched = true;
      sgdbGameId = found[0] === undefined ? null : String(found[0].id);
      images = [];
      if (sgdbGameId !== null) await loadImages(current);
    } catch (cause) {
      if (!destroyed && current === request) error = toMessage(cause);
    } finally {
      if (!destroyed && current === request) loading = false;
    }
  }

  async function loadImages(current = ++request): Promise<void> {
    if (sgdbGameId === null) return;
    loading = true;
    error = null;
    try {
      const found = await listSteamGridDbArtwork(Number(sgdbGameId), tab);
      if (!destroyed && current === request) images = found;
    } catch (cause) {
      if (!destroyed && current === request) { error = toMessage(cause); images = []; }
    } finally {
      if (!destroyed && current === request) loading = false;
    }
  }

  async function apply(image: SteamGridDbImage): Promise<void> {
    if (applying) return;
    applying = true;
    error = null;
    try {
      await applySteamGridDbArtwork(game.id, activeTab.slot, image.url);
      if (destroyed) return;
      showToast(t("Immagine salvata.", $language), "success");
      onClose();
    } catch (cause) {
      if (!destroyed) error = toMessage(cause);
    } finally {
      if (!destroyed) applying = false;
    }
  }
</script>

<Dialog open title={t("Scegli da SteamGridDB", $language)} size="large" {onClose}>
    {#if view === "no-key"}
      <EmptyState icon="link" title={t("Chiave API mancante", $language)} description={t("Inserisci la tua chiave API di SteamGridDB nelle impostazioni generali per cercare le immagini.", $language)} />
    {:else}
      <form class="flex items-end gap-2" onsubmit={(event) => { event.preventDefault(); void search(); }}>
        <TextField id="steamgriddb-query" type="search" label={t("Cerca per nome", $language)} bind:value={query} disabled={loading || applying} class="min-w-0 flex-1" />
        <Button type="submit" label={t("Cerca", $language)} variant="secondary" disabled={loading || applying || query.trim().length === 0} />
      </form>
      {#if games.length > 1}
        <SelectField id="steamgriddb-game" label={t("Gioco trovato", $language)} value={sgdbGameId ?? ""} options={games.map((entry) => ({ value: String(entry.id), label: entry.name }))}
          disabled={loading || applying} onChange={(value) => { sgdbGameId = value; void loadImages(); }} />
      {/if}
      <SegmentedControl name="steamgriddb-tab" label={t("Tipo di immagine", $language)} labelHidden
        options={steamGridDbTabs.map((entry) => ({ value: entry.id, label: t(entry.label, $language) }))} value={tab} disabled={applying}
        onChange={(value) => { tab = value as typeof tab; void loadImages(); }} />
      {#if view === "loading"}
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-3" role="status" aria-label={t("Caricamento...", $language)}>
          {#each [0, 1, 2, 3, 4, 5] as slot (slot)}<Skeleton class="aspect-video w-full" />{/each}
        </div>
      {:else if view === "error"}
        <EmptyState icon="warning" title={t("Impossibile caricare le immagini", $language)} description={error ?? ""}>
          <Button label={t("Riprova", $language)} variant="secondary" onClick={() => void (games.length === 0 ? search() : loadImages())} />
        </EmptyState>
      {:else if view === "no-games"}
        <EmptyState icon="search" title={t("Nessun gioco trovato", $language)} description={t("Prova con un nome diverso.", $language)} />
      {:else if view === "no-images"}
        <EmptyState icon="search" title={t("Nessuna immagine disponibile", $language)} description={t("SteamGridDB non ha immagini di questo tipo per il gioco.", $language)} />
      {:else}
        <ul class="grid max-h-[28rem] grid-cols-2 gap-4 overflow-y-auto p-1 sm:grid-cols-3">
          {#each images as image (image.id)}
            <li>
              <button type="button" class="block w-full overflow-hidden rounded-card bg-legio-fill ring-1 ring-legio-line-subtle transition hover:ring-legio-line-strong disabled:opacity-50"
                disabled={applying} aria-label={t("Usa questa immagine ({0} x {1})", $language, [image.width, image.height])} onclick={() => void apply(image)}>
                <img src={image.thumbUrl} alt="" loading="lazy" referrerpolicy="no-referrer" draggable="false" class="aspect-video w-full object-contain" />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
</Dialog>
