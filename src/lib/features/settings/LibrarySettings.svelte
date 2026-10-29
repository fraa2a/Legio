<script lang="ts">
  import { onMount } from "svelte";
  import { language, t } from "../../i18n";
  import { repairGameLibrary, type GameLibrary } from "../../services/game-libraries";
  import { gameLibraries, pickAndAddLibrary } from "../../stores/game-libraries";
  import { hiddenSteamGames, restoreSteamGame } from "../../stores/games";
  import { toMessage } from "../../utils/errors";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Badge from "../../components/ui/Badge.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { formatLibraryDetails } from "../library/library-details";
  import RemoveLibraryDialog from "./RemoveLibraryDialog.svelte";

  let pending = $state(false);
  let error = $state<string | null>(null);
  let restoringId = $state<string | null>(null);
  let removing = $state<GameLibrary | null>(null);
  onMount(() => { void gameLibraries.reload(); void hiddenSteamGames.load(); });

  async function restore(id: string): Promise<void> {
    restoringId = id;
    error = null;
    try { await restoreSteamGame(id); }
    catch (cause) { error = toMessage(cause); }
    finally { restoringId = null; }
  }

  async function addLibrary(): Promise<void> {
    pending = true;
    error = null;
    try { await pickAndAddLibrary(); }
    catch (cause) { error = toMessage(cause); }
    finally { pending = false; }
  }

  async function repair(library: GameLibrary): Promise<void> {
    pending = true;
    error = null;
    try { await repairGameLibrary(library.root); await gameLibraries.reload(); }
    catch (cause) { error = toMessage(cause); }
    finally { pending = false; }
  }
</script>

<SettingsGroup icon="folder" title={t("Librerie dei giochi", $language)}>
  <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Le librerie sono le cartelle in cui Legio installa i giochi; a ogni download scegli quella da usare. Aggiungendo una cartella, Legio crea al suo interno la sottocartella Legio. Se la cartella contiene già un catalogo library.ldf, i suoi giochi vengono importati.", $language)}</p>
  {#if $gameLibraries.error !== null}<ErrorBanner message={$gameLibraries.error} onRetry={() => void gameLibraries.reload()} />{/if}
  {#each $gameLibraries.data as library (library.id)}
    <div class="flex flex-col gap-2 rounded-lg bg-white/5 p-3 light:bg-zinc-900/5">
      <div class="flex flex-wrap items-center gap-3">
        <span class="shrink-0 text-zinc-400 light:text-zinc-600"><Icon name="folder" /></span>
        <div class="flex min-w-0 flex-1 flex-col gap-0.5">
          <div class="flex min-w-0 items-center gap-2">
            <p class="break-all text-sm text-zinc-200 light:text-zinc-800">{library.root}</p>
            {#if library.isDefault}<Badge title={t("Predefinita", $language)} />{/if}
          </div>
          <p class="text-xs text-zinc-400 light:text-zinc-600">{formatLibraryDetails(library, $language)}</p>
        </div>
        {#if !library.isDefault}
          <Button label={t("Rimuovi...", $language)} variant="danger-soft" size="sm" disabled={pending} onClick={() => (removing = library)} />
        {/if}
      </div>
      {#if library.warning}
        <ErrorBanner message={library.warning} tone="warning" />
        <p class="text-xs text-zinc-400 light:text-zinc-600">{t("La riparazione conserva una copia del catalogo e lo ricostruisce dai dati locali.", $language)}</p>
        <div><Button label={t("Ripara catalogo", $language)} variant="secondary" disabled={pending} onClick={() => void repair(library)} /></div>
      {/if}
    </div>
  {/each}
  <div>
    <Button label={t("Aggiungi libreria...", $language)} variant="secondary" disabled={pending} onClick={() => void addLibrary()}>
      <Icon name="plus" size="size-4" />
    </Button>
  </div>
  {#if error}<ErrorBanner message={error} />{/if}
</SettingsGroup>

{#if removing !== null}
  <RemoveLibraryDialog library={removing} onClose={() => (removing = null)} />
{/if}

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
