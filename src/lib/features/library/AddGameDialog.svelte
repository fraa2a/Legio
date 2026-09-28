<script lang="ts">
  import { untrack } from "svelte";
  import { toMessage } from "../../utils/errors";
  import { addGame } from "../../stores/games";
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
  import { previewManualGameSteamAppId, type SteamIdentificationPreview } from "../../services/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import SteamArtwork from "./SteamArtwork.svelte";

  let { onClose }: { onClose: () => void } = $props();

  let steamAppId = $state("");
  let showSteamOnly = $state(false);
  let preview = $state<SteamIdentificationPreview | null>(null);
  let previewPending = $state(false);
  let previewError = $state<string | null>(null);
  let pending = $state(false);
  let added = $state(false);
  let actionError = $state<string | null>(null);
  let previewRequest = 0;

  const selectedPath = $derived($manualImport.selectedPath);
  const parsedAppId = $derived(parseAppId(steamAppId));
  const detailsState = $derived(parsedAppId === null ? null : ($steamDetails[parsedAppId] ?? null));
  const details = $derived(detailsState?.details ?? null);
  const selectedCandidate = $derived(preview?.candidates.find((candidate) => candidate.steamAppId === parsedAppId) ?? null);
  const canAdd = $derived(
    $manualImport.gameName.trim().length > 0 && (selectedPath !== null || parsedAppId !== null) &&
    (steamAppId.trim().length === 0 || parsedAppId !== null) && !previewPending && !pending && !added,
  );

  function parseAppId(value: string): number | null {
    const trimmed = value.trim();
    if (!/^[1-9]\d*$/.test(trimmed)) return null;
    const parsed = Number(trimmed);
    return Number.isSafeInteger(parsed) && parsed <= 4_294_967_295 ? parsed : null;
  }

  function fileStem(path: string): string {
    return path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "";
  }

  $effect(() => {
    const path = selectedPath;
    const request = ++previewRequest;
    preview = null;
    previewError = null;
    steamAppId = "";
    if (path === null) {
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
          if (steamAppId.trim().length === 0) steamAppId = String(candidate.steamAppId);
          if (untrack(() => $manualImport.gameName) === fallbackName) setScanGameName(candidate.name);
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
    if (parsedAppId !== null) ensureSteamDetails(parsedAppId);
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
          name: details?.name ?? selectedCandidate?.name ?? name,
        };
        const result = await importScannedGame(name, identity);
        if (result.linkingError !== null) {
          added = true;
          actionError = `Gioco aggiunto. Collegamento Steam non riuscito: ${result.linkingError}`;
          return;
        }
      } else if (parsedAppId !== null) {
        await addGame({ name, steamAppId: parsedAppId });
      }
      close();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }

  function close(): void {
    resetManualImport();
    onClose();
  }
</script>

<Dialog open title="Aggiungi un gioco" size="wide" onClose={close}>
  <div class="flex flex-col gap-5">
    <TextField
      id="add-game-name"
      label="Nome gioco"
      value={$manualImport.gameName}
      placeholder="Nome da mostrare in libreria"
      disabled={pending || added}
      oninput={setScanGameName}
    />

    <div class="flex flex-col gap-2">
      <span class="text-sm text-zinc-400 light:text-zinc-600">Eseguibile</span>
      <div class="flex flex-wrap items-center gap-2 rounded-xl border border-white/10 bg-white/5 p-3 light:border-zinc-900/10 light:bg-white">
        <span class="min-w-0 flex-1 break-all text-sm text-zinc-200 light:text-zinc-800">
          {selectedPath ?? "Nessun file selezionato"}
        </span>
        <Button label="Scegli .exe" variant="secondary" disabled={pending || added} onClick={() => void pickExecutable()} />
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Button label="Scansiona cartella" variant="secondary" disabled={pending || added} onClick={() => void browseGameDirectory()} />
        {#if $manualImport.directory !== null}
          <Button label="Ripeti scansione" variant="secondary" disabled={pending || added} onClick={() => void rescanCurrentDirectory()} />
        {/if}
        <span class="text-xs text-zinc-500">Puoi scegliere il file direttamente o cercarlo nella cartella del gioco.</span>
      </div>
      {#if $manualImport.status === "loading"}
        <p class="text-sm text-zinc-400" role="status">Scansione degli eseguibili in corso...</p>
      {:else if $manualImport.status === "empty"}
        <p class="text-sm text-zinc-400">Nessun eseguibile trovato nella cartella.</p>
      {:else if $manualImport.status === "error" && $manualImport.error !== null}
        <ErrorBanner message={$manualImport.error} onRetry={() => void rescanCurrentDirectory()} />
      {/if}
      {#if $manualImport.candidates.length > 0}
        <fieldset class="max-h-40 space-y-1 overflow-y-auto rounded-lg bg-white/5 p-2 light:bg-zinc-100" disabled={pending || added}>
          <legend class="sr-only">Eseguibili trovati</legend>
          {#each $manualImport.candidates as candidate (candidate.path)}
            <label class="flex cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-sm text-zinc-200 hover:bg-white/10 light:text-zinc-800 light:hover:bg-zinc-200">
              <input type="radio" name="add-game-executable" checked={selectedPath === candidate.path} onchange={() => chooseCandidate(candidate.path)} class="size-4 accent-white" />
              <span class="min-w-0 truncate" title={candidate.path}>{candidate.path}</span>
            </label>
          {/each}
        </fieldset>
      {/if}
    </div>

    {#if selectedPath !== null || showSteamOnly}
      <div class="flex flex-col gap-2">
        <TextField id="add-steam-app-id" label="Steam App ID" value={steamAppId} inputmode="numeric" placeholder="Opzionale" disabled={pending || added} oninput={(value) => (steamAppId = value)} />
        {#if previewPending}
          <p class="text-xs text-zinc-400" role="status">Rilevamento del gioco su Steam...</p>
        {:else if previewError !== null}
          <p class="text-xs text-amber-300 light:text-amber-800">Rilevamento non disponibile: {previewError}</p>
        {:else if preview?.status === "no_match"}
          <p class="text-xs text-zinc-500">Nessuna corrispondenza esatta. Puoi inserire l'App ID manualmente.</p>
        {:else if preview?.status === "unavailable"}
          <p class="text-xs text-amber-300 light:text-amber-800">Rilevamento non disponibile: {preview.message ?? "riprova più tardi"}.</p>
        {:else if preview?.status === "matched"}
          <p class="text-xs text-emerald-300 light:text-emerald-700">App ID rilevato automaticamente. Puoi correggerlo prima di aggiungere il gioco.</p>
        {/if}
        {#if steamAppId.trim().length > 0 && parsedAppId === null}
          <p class="text-xs text-red-300 light:text-red-700">Inserisci un App ID Steam valido.</p>
        {/if}
        {#if preview?.status === "ambiguous"}
          <div class="flex flex-wrap gap-2" aria-label="Corrispondenze Steam">
            {#each preview.candidates as candidate (candidate.steamAppId)}
              <Button label={`${candidate.name} (${candidate.steamAppId})`} variant="secondary" disabled={pending || added} onClick={() => { steamAppId = String(candidate.steamAppId); setScanGameName(candidate.name); }} />
            {/each}
          </div>
        {/if}
        {#if parsedAppId !== null}
          <div class="flex items-center gap-3 rounded-xl border border-white/10 bg-white/5 p-2 light:border-zinc-900/10 light:bg-white">
            <div class="h-16 w-32 shrink-0 overflow-hidden rounded-lg bg-zinc-800 light:bg-zinc-200">
              {#if details !== null}
                <SteamArtwork steamAppId={parsedAppId} asset="header" version={detailsState?.cachedAt ?? null} caption={false} class="size-full object-cover" />
              {/if}
            </div>
            <div class="min-w-0">
              <p class="truncate text-sm font-medium text-zinc-100 light:text-zinc-900">{details?.name ?? selectedCandidate?.name ?? "Anteprima Steam"}</p>
              <p class="text-xs text-zinc-500">App ID {parsedAppId}</p>
              {#if detailsState?.status === "error"}<p class="text-xs text-amber-300 light:text-amber-800">Copertina non disponibile.</p>{/if}
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <button type="button" class="self-start text-sm text-zinc-400 underline-offset-2 hover:text-zinc-100 hover:underline light:text-zinc-600 light:hover:text-zinc-900" onclick={() => (showSteamOnly = true)}>
        Non hai un eseguibile? Aggiungi tramite App ID Steam
      </button>
    {/if}

    {#if actionError !== null}<ErrorBanner message={actionError} />{/if}
    <div class="flex justify-end gap-2">
      <Button label={added ? "Chiudi" : "Annulla"} variant="secondary" onClick={close} />
      {#if !added}<Button label={pending ? "Aggiunta in corso..." : "Aggiungi alla libreria"} disabled={!canAdd} onClick={() => void add()} />{/if}
    </div>
  </div>
</Dialog>
