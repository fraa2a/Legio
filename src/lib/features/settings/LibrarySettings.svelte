<script lang="ts">
  import { onMount } from "svelte";
  import { language, t } from "../../i18n";
  import { pickGameDirectory } from "../../services/dialog";
  import { listGameLibraries, importGameLibrary, repairGameLibrary, type GameLibrary } from "../../services/game-libraries";
  import { games, hiddenSteamGames, restoreSteamGame } from "../../stores/games";
  import { toMessage } from "../../utils/errors";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";

  let libraries = $state<GameLibrary[]>([]);
  let pending = $state(false);
  let error = $state<string | null>(null);
  let restoringId = $state<string | null>(null);
  onMount(() => { void refresh(); void hiddenSteamGames.load(); });

  async function restore(id: string): Promise<void> {
    restoringId = id;
    error = null;
    try { await restoreSteamGame(id); }
    catch (cause) { error = toMessage(cause); }
    finally { restoringId = null; }
  }

  async function refresh(): Promise<void> {
    try { libraries = await listGameLibraries(); }
    catch (cause) { error = toMessage(cause); }
  }

  async function importLibrary(): Promise<void> {
    pending = true;
    error = null;
    try {
      const directory = await pickGameDirectory("");
      if (directory !== null) {
        await importGameLibrary(directory);
        await games.reload();
        await refresh();
      }
    } catch (cause) { error = toMessage(cause); }
    finally { pending = false; }
  }

  async function repair(library: GameLibrary): Promise<void> {
    pending = true;
    error = null;
    try { await repairGameLibrary(library.root); await refresh(); }
    catch (cause) { error = toMessage(cause); }
    finally { pending = false; }
  }
</script>

<SettingsGroup icon="folder" title={t("Librerie dei giochi", $language)}>
  <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Importa una cartella contenente library.ldf. I giochi restano nella posizione attuale.", $language)}</p>
  {#each libraries as library (library.id)}
    <div class="flex flex-col gap-2 rounded-lg bg-white/5 p-3 light:bg-zinc-900/5">
      <p class="break-all text-sm text-zinc-200 light:text-zinc-800">{library.root}</p>
      {#if library.warning}
        <ErrorBanner message={library.warning} tone="warning" />
        <p class="text-xs text-zinc-400 light:text-zinc-600">{t("La riparazione conserva una copia del catalogo e lo ricostruisce dai dati locali.", $language)}</p>
        <div><Button label={t("Ripara catalogo", $language)} variant="secondary" disabled={pending} onClick={() => void repair(library)} /></div>
      {/if}
    </div>
  {/each}
  <div><Button label={t("Importa libreria...", $language)} variant="secondary" disabled={pending} onClick={() => void importLibrary()} /></div>
  {#if error}<ErrorBanner message={error} />{/if}
</SettingsGroup>

<SettingsGroup icon="store" title={t("Giochi Steam nascosti", $language)}>
  {#if $hiddenSteamGames.error !== null}
    <ErrorBanner message={$hiddenSteamGames.error} onRetry={() => void hiddenSteamGames.reload()} />
  {/if}
  {#if $hiddenSteamGames.data.length === 0}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Nessun gioco Steam nascosto.", $language)}</p>
  {:else}
    {#each $hiddenSteamGames.data as game (game.id)}
      <div class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-white/5 p-3 light:bg-zinc-900/5">
        <span class="min-w-0 text-sm text-zinc-200 light:text-zinc-800">{game.name}</span>
        <Button label={t("Mostra nella libreria", $language)} variant="secondary" disabled={restoringId !== null} onClick={() => void restore(game.id)} />
      </div>
    {/each}
  {/if}
  {#if error}<ErrorBanner message={error} />{/if}
</SettingsGroup>
