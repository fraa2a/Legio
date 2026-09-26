<script lang="ts">
  import { importSteamLibrary, resetSteamLibrary, scanSteamLibrary, steamLibrary } from "../../stores/steam-library";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";

  let { onClose }: { onClose: () => void } = $props();

  const result = $derived($steamLibrary.importResult);

  function close(): void {
    resetSteamLibrary();
    onClose();
  }
</script>

<Dialog open title="Rilevamento installazioni Steam" size="wide" onClose={close}>
  {#if $steamLibrary.status === "loading"}
    <p class="flex items-center gap-3 text-sm text-zinc-300 light:text-zinc-700" role="status">
      <span
        class="size-4 shrink-0 animate-spin rounded-full border-2 border-zinc-400 border-t-transparent"
        aria-hidden="true"
      ></span>
      Scansione delle installazioni Steam in corso...
    </p>
  {/if}

  {#if $steamLibrary.error !== null}
    <ErrorBanner message={$steamLibrary.error} onRetry={() => void scanSteamLibrary()} />
  {/if}

  {#if $steamLibrary.scan !== null}
    <section class="flex flex-col gap-3">
      <p class="text-sm text-zinc-400 light:text-zinc-600">
        {$steamLibrary.scan.games.length} giochi rilevati. La preview non modifica la libreria:
        l'importazione richiede una conferma esplicita e riesegue la scansione.
      </p>
      <ul class="flex max-h-64 flex-col gap-2 overflow-y-auto">
        {#each $steamLibrary.scan.games as game (game.appId)}
          <li class="flex flex-col gap-1 rounded-lg bg-white/5 p-3 light:bg-zinc-100">
            <p class="text-sm font-medium text-zinc-50 light:text-zinc-900">{game.name}</p>
            <p class="break-all text-xs text-zinc-500">App ID {game.appId}</p>
            <p class="break-all text-xs text-zinc-500">{game.installPath}</p>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if $steamLibrary.scan?.diagnostics.length}
    <section class="flex flex-col gap-2">
      <h3 class="text-sm font-semibold text-zinc-200 light:text-zinc-800">Diagnostica</h3>
      <ul class="flex flex-col gap-1 text-xs text-amber-300 light:text-amber-800">
        {#each $steamLibrary.scan.diagnostics as diagnostic (diagnostic)}
          <li>{diagnostic}</li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if result !== null}
    <section class="flex flex-col gap-3">
      <h3 class="text-sm font-semibold text-zinc-200 light:text-zinc-800">Importazione completata</h3>
      <dl class="grid grid-cols-2 gap-2 text-sm sm:grid-cols-5">
        <div>
          <dt class="text-xs text-zinc-500">Rilevati</dt>
          <dd class="text-zinc-100 light:text-zinc-900">{result.detected}</dd>
        </div>
        <div>
          <dt class="text-xs text-zinc-500">Inseriti</dt>
          <dd class="text-zinc-100 light:text-zinc-900">{result.inserted}</dd>
        </div>
        <div>
          <dt class="text-xs text-zinc-500">Aggiornati</dt>
          <dd class="text-zinc-100 light:text-zinc-900">{result.updated}</dd>
        </div>
        <div>
          <dt class="text-xs text-zinc-500">Invariati</dt>
          <dd class="text-zinc-100 light:text-zinc-900">{result.unchanged}</dd>
        </div>
        <div>
          <dt class="text-xs text-zinc-500">Rimossi</dt>
          <dd class="text-zinc-100 light:text-zinc-900">{result.removed}</dd>
        </div>
      </dl>
      {#if result.diagnostics.length > 0}
        <ul class="flex flex-col gap-1 text-xs text-amber-300 light:text-amber-800">
          {#each result.diagnostics as diagnostic (diagnostic)}
            <li>{diagnostic}</li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}

  <div class="flex flex-wrap justify-end gap-2">
    <Button label="Chiudi" variant="secondary" onClick={close} />
    {#if result === null}
      <Button
        label="Riesegui scansione"
        variant="secondary"
        disabled={$steamLibrary.importing}
        onClick={() => void scanSteamLibrary()}
      />
      <Button
        label={$steamLibrary.importing ? "Importazione..." : "Importa e aggiorna"}
        disabled={$steamLibrary.importing}
        onClick={() => void importSteamLibrary()}
      />
    {:else}
      <Button
        label="Rileva di nuovo"
        variant="secondary"
        onClick={() => void scanSteamLibrary()}
      />
    {/if}
  </div>
</Dialog>
