<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount, untrack } from "svelte";
  import { toMessage } from "../../utils/errors";
  import { addGame, games } from "../../stores/games";
  import {
    browseGameDirectory,
    chooseCandidate,
    importScannedGame,
    manualImport,
    pickExecutable,
    resetManualImport,
    rescanCurrentDirectory,
    setScanGameName,
  } from "../../stores/manual-import";
  import { ensureSteamDetails, steamDetails } from "../../stores/steam-details";
  import { cachedCatalogSearch, refreshCatalogCached, searchCatalog, type CatalogGame } from "../../services/catalog";
  import { previewManualGameSteamAppId, type SteamIdentificationPreview } from "../../services/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";
  import { showToast } from "../../stores/toast";

  let { onClose, prefill = null }: { onClose: () => void; prefill?: { steamAppId: number; name: string; suggestedName: string } | null } = $props();

  let selectedSteamGame = $state<{ steamAppId: number; name: string } | null>(null);
  let steamQuery = $state("");
  let searchResults = $state<CatalogGame[]>([]);
  let searchStatus = $state<"idle" | "loading" | "ready" | "empty">("idle");
  let searchError = $state<string | null>(null);
  let preview = $state<SteamIdentificationPreview | null>(null);
  let previewPending = $state(false);
  let previewError = $state<string | null>(null);
  let pending = $state(false);
  let actionError = $state<string | null>(null);
  let previewRequest = 0;
  let searchRequest = 0;

  onMount(() => {
    if (prefill !== null) {
      selectedSteamGame = prefill;
      setScanGameName(prefill.suggestedName);
    }
  });

  const selectedPath = $derived($manualImport.selectedPath);
  const parsedAppId = $derived(selectedSteamGame?.steamAppId ?? null);
  const detailsState = $derived(parsedAppId === null ? null : ($steamDetails[parsedAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const duplicateName = $derived(parsedAppId !== null && $games.data.some((game) =>
    game.steamAppId === parsedAppId && game.name.trim().toLocaleLowerCase() === $manualImport.gameName.trim().toLocaleLowerCase()));
  const canAdd = $derived(
    $manualImport.gameName.trim().length > 0 && (selectedPath !== null || parsedAppId !== null) &&
    !duplicateName && !previewPending && !pending,
  );

  function fileStem(path: string): string {
    return path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "";
  }

  function selectSteamGame(game: { steamAppId: number; name: string }): void {
    const currentName = $manualImport.gameName.trim();
    if (currentName.length === 0 || (selectedPath !== null && currentName === fileStem(selectedPath))) {
      setScanGameName(game.name);
    }
    selectedSteamGame = { steamAppId: game.steamAppId, name: game.name };
    steamQuery = "";
  }

  $effect(() => {
    const path = selectedPath;
    const request = ++previewRequest;
    preview = null;
    previewError = null;
    if (path === null) {
      previewPending = false;
      return;
    }
    if (prefill !== null) {
      previewPending = false;
      return;
    }
    const fallbackName = fileStem(path);
    if (untrack(() => $manualImport.gameName.trim().length) === 0) setScanGameName(fallbackName);
    previewPending = true;
    void previewManualGameSteamAppId(path).then(
      (result) => {
        if (request !== previewRequest) return;
        preview = result;
        if (result.status === "matched") {
          const candidate = result.candidates[0];
          if (candidate !== undefined && selectedSteamGame === null) selectSteamGame(candidate);
        }
      },
      (error: unknown) => {
        if (request === previewRequest) previewError = toMessage(error);
      },
    ).finally(() => {
      if (request === previewRequest) previewPending = false;
    });
  });

  $effect(() => {
    void $language;
    if (parsedAppId !== null) ensureSteamDetails(parsedAppId);
  });

  $effect(() => {
    const query = steamQuery.trim();
    const request = ++searchRequest;
    searchResults = [];
    searchError = null;
    if (query.length < 2) {
      searchStatus = "idle";
      return;
    }
    const known = cachedCatalogSearch(query);
    if (known !== null) {
      searchResults = known.games;
      searchStatus = known.games.length > 0 ? "ready" : "empty";
      return;
    }
    searchStatus = "loading";
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const cached = await searchCatalog(query);
          if (request !== searchRequest) return;
          searchResults = cached.games;
          searchStatus = cached.games.length > 0 ? "ready" : "empty";
          const fresh = await refreshCatalogCached(query);
          if (request !== searchRequest) return;
          searchResults = fresh.games;
          searchStatus = fresh.games.length > 0 ? "ready" : "empty";
        } catch (error) {
          if (request !== searchRequest) return;
          searchError = toMessage(error);
          if (searchResults.length === 0) searchStatus = "empty";
        }
      })();
    }, 300);
    return () => clearTimeout(timer);
  });

  async function add(): Promise<void> {
    if (!canAdd) return;
    actionError = null;
    pending = true;
    try {
      const name = $manualImport.gameName.trim();
      if (selectedPath !== null) {
        const identity = parsedAppId === null ? null : {
          steamAppId: parsedAppId,
          name: details?.name ?? selectedSteamGame?.name ?? name,
        };
        const result = await importScannedGame(name, identity);
        if (result.linkingError !== null || result.shortcutWarning !== null) {
          const warning = [
            result.linkingError === null ? null : t("Gioco aggiunto. Collegamento Steam non riuscito: {0}", $language, [result.linkingError]),
            result.shortcutWarning === null ? null : t("Gioco aggiunto. Avviso sull'icona: {0}", $language, [result.shortcutWarning]),
          ].filter((message) => message !== null).join(" ");
          showToast(warning, "error");
        }
      } else if (parsedAppId !== null) {
        await addGame({ name, steamAppId: parsedAppId });
      }
      showToast(t("Gioco aggiunto alla libreria.", $language), "success");
      close();
    } catch (error) {
      showToast(toMessage(error), "error");
    } finally {
      pending = false;
    }
  }

  function close(): void {
    resetManualImport();
    onClose();
  }
</script>

<Dialog open title={t("Aggiungi un gioco", $language)} size="wide" onClose={close}>
  {#snippet children(dismiss)}
  <div class="flex flex-col gap-5">
    <div class="flex flex-col gap-2">
        <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Associa il gioco corretto su Steam se il rilevamento automatico è errato o non trova il gioco.", $language)}</p>
        <TextField id="add-steam-game-search" label={t("Cerca gioco su Steam", $language)} type="search" value={steamQuery} placeholder={t("Cerca per nome", $language)} disabled={pending}
          oninput={(value) => { steamQuery = value; selectedSteamGame = null; }} />
        {#if searchStatus === "loading"}
          <p class="text-xs text-zinc-400" role="status">{t("Ricerca in corso...", $language)}</p>
        {:else if searchStatus === "empty" && searchError === null}
          <p class="text-xs text-zinc-500">{t("Nessun risultato per questa ricerca.", $language)}</p>
        {/if}
        {#if searchError !== null}<p class="text-xs text-amber-300 light:text-amber-800">{t("Ricerca non disponibile: ", $language)}{searchError}</p>{/if}
        {#if searchResults.length > 0 && steamQuery.trim().length >= 2}
          <ul class="max-h-36 overflow-y-auto rounded-lg border border-white/10 bg-white/5 light:border-zinc-900/10 light:bg-white" aria-label={t("Risultati Steam", $language)}>
            {#each searchResults as result (result.steamAppId)}
              <li>
                <button type="button" class="flex h-12 w-full items-center px-3 text-left text-sm text-zinc-200 hover:bg-white/10 focus-visible:outline focus-visible:outline-2 focus-visible:outline-white light:text-zinc-800 light:hover:bg-zinc-900/5 light:focus-visible:outline-zinc-900"
                  disabled={pending} onclick={() => selectSteamGame(result)}>
                  <span class="truncate">{result.name}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if previewPending}
          <p class="text-xs text-zinc-400" role="status">{t("Rilevamento del gioco su Steam...", $language)}</p>
        {:else if previewError !== null}
          <p class="text-xs text-amber-300 light:text-amber-800">{t("Rilevamento non disponibile: ", $language)}{previewError}</p>
        {:else if preview?.status === "no_match"}
          <p class="text-xs text-zinc-500">{t("Nessuna corrispondenza esatta. Cerca il gioco per nome.", $language)}</p>
        {:else if preview?.status === "unavailable"}
          <p class="text-xs text-amber-300 light:text-amber-800">{t("Rilevamento non disponibile: ", $language)}{preview.message ?? t("riprova più tardi", $language)}.</p>
        {:else if preview?.status === "matched"}
          <p class="text-xs text-emerald-300 light:text-emerald-700">{t("Gioco Steam rilevato automaticamente. Puoi cambiarlo con la ricerca.", $language)}</p>
        {/if}
        {#if preview?.status === "ambiguous"}
          <div class="flex flex-wrap gap-2" aria-label={t("Corrispondenze Steam", $language)}>
            {#each preview.candidates as candidate (candidate.steamAppId)}
              <Button label={candidate.name} variant="secondary" disabled={pending} onClick={() => selectSteamGame(candidate)} />
            {/each}
          </div>
        {/if}
        {#if parsedAppId !== null}
          <div class="flex items-center gap-3 rounded-xl border border-white/10 bg-white/5 p-2 light:border-zinc-900/10 light:bg-white">
            <div class="h-16 w-32 shrink-0 overflow-hidden rounded-lg bg-zinc-800 light:bg-zinc-200">
              <SteamArtwork steamAppId={parsedAppId} asset="header" version={detailsState?.cachedAt ?? null} caption={false} alt="" class="size-full object-cover" />
            </div>
            <div class="min-w-0">
              <p class="truncate text-sm font-medium text-zinc-100 light:text-zinc-900">{details?.name ?? selectedSteamGame?.name ?? t("Gioco Steam", $language)}</p>
              <p class="text-xs text-zinc-500">{t("Gioco selezionato", $language)}</p>
              {#if detailsState?.status === "error"}<p class="text-xs text-amber-300 light:text-amber-800">{t("Copertina non disponibile.", $language)}</p>{/if}
            </div>
          </div>
        {/if}
      </div>

    <TextField
      id="add-game-name"
      label={t("Nome gioco", $language)}
      value={$manualImport.gameName}
      placeholder={t("Nome da mostrare in libreria", $language)}
      disabled={pending}
      oninput={setScanGameName}
    />
    {#if duplicateName}<p class="text-sm text-amber-300 light:text-amber-800" role="alert">{t("Il nome è già usato per questo gioco Steam. Scegli un nome diverso.", $language)}</p>{/if}

    <div class="flex flex-col gap-2 border-t border-white/10 pt-4 light:border-zinc-900/10">
      <span class="text-sm font-medium text-zinc-100 light:text-zinc-900">{t("Eseguibile del gioco", $language)}</span>
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Scegli il file del gioco oppure scansiona la sua cartella. Puoi aggiungerlo anche senza eseguibile se hai selezionato un gioco Steam.", $language)}</p>
      <div class="flex flex-wrap items-center gap-2">
        <Button label={t("Scegli .exe", $language)} variant="secondary" disabled={pending} onClick={() => void pickExecutable()} />
        <Button label={t("Scansiona cartella", $language)} variant="secondary" disabled={pending} onClick={() => void browseGameDirectory()} />
        {#if $manualImport.directory !== null}
          <Button label={t("Ripeti scansione", $language)} variant="secondary" disabled={pending} onClick={() => void rescanCurrentDirectory()} />
        {/if}
      </div>
      <p class="min-h-5 break-all text-xs text-zinc-400 light:text-zinc-600">{selectedPath ?? t("Nessun file selezionato", $language)}</p>
      {#if $manualImport.status === "loading"}
        <p class="text-sm text-zinc-400" role="status">{t("Scansione degli eseguibili in corso...", $language)}</p>
      {:else if $manualImport.status === "empty"}
        <p class="text-sm text-zinc-400">{t("Nessun eseguibile trovato nella cartella.", $language)}</p>
      {:else if $manualImport.status === "error" && $manualImport.error !== null}
        <ErrorBanner message={$manualImport.error} onRetry={() => void rescanCurrentDirectory()} />
      {/if}
      {#if $manualImport.candidates.length > 0}
        <fieldset class="max-h-40 space-y-1 overflow-y-auto rounded-lg bg-white/5 p-2 light:bg-zinc-100" disabled={pending}>
          <legend class="sr-only">{t("Eseguibili trovati", $language)}</legend>
          {#each $manualImport.candidates as candidate (candidate.path)}
            <label class="flex cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-sm text-zinc-200 hover:bg-white/10 light:text-zinc-800 light:hover:bg-zinc-200">
              <input type="radio" name="add-game-executable" checked={selectedPath === candidate.path} onchange={() => chooseCandidate(candidate.path)} class="size-4 accent-white" />
              <span class="min-w-0 truncate" title={candidate.path}>{candidate.path}</span>
            </label>
          {/each}
        </fieldset>
      {/if}
    </div>

    {#if actionError !== null}<ErrorBanner message={actionError} />{/if}
    <div class="flex justify-end gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" onClick={dismiss} />
      <Button label={pending ? t("Aggiunta in corso...", $language) : t("Aggiungi alla libreria", $language)} disabled={!canAdd} onClick={() => void add()} />
    </div>
  </div>
  {/snippet}
</Dialog>
