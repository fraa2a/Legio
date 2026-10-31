<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import type { Game } from "../../services/local-state";
  import { launchStateByGame } from "../../stores/launch";
  import { bindGame, loadModdableGames, moddableGameById, moddableGames, restoreGameFolder } from "../../stores/modding";
  import { openModManager } from "../../stores/navigation";
  import { toMessage } from "../../utils/errors";
  import EcosystemGameDialog from "./EcosystemGameDialog.svelte";
  import { describeModdingError, moddableGameState } from "./modding-model";

  let { game }: { game: Game } = $props();

  onMount(() => void loadModdableGames());

  const modding = $derived($moddableGameById.get(game.id) ?? null);
  const modState = $derived(modding === null ? null : moddableGameState(modding));
  const running = $derived(($launchStateByGame.get(game.id)?.status ?? "idle") !== "idle");

  let pickerOpen = $state(false);
  let restoring = $state(false);
  let restored = $state(false);
  let restoreError = $state<string | null>(null);

  const bindingText = $derived.by(() => {
    if (modding === null) return "";
    if (modding.binding !== null) return t("Collegato a {0}.", $language, [modding.displayName ?? modding.binding.ecosystemGameId]);
    if (modding.ecosystemGameId !== null) return t("Corrispondenza automatica: {0}.", $language, [modding.displayName ?? modding.ecosystemGameId]);
    return t("Thunderstore non ha riconosciuto questo gioco. Collegalo a mano per installare le sue mod.", $language);
  });

  async function restore(): Promise<void> {
    restoring = true;
    restored = false;
    restoreError = null;
    try {
      await restoreGameFolder(game.id);
      restored = true;
    } catch (error) {
      restoreError = describeModdingError(toMessage(error), $language);
    } finally {
      restoring = false;
    }
  }
</script>

<div class="flex flex-col gap-4">
  <StateBlock
    status={$moddableGames.status === "ready" || $moddableGames.status === "empty" ? "ready" : $moddableGames.status}
    hasData={modding !== null}
    loadingMessage={t("Caricamento...", $language)}
    error={$moddableGames.error}
    onRetry={() => void moddableGames.reload()}
  />
  {#if modding === null && ($moddableGames.status === "ready" || $moddableGames.status === "empty")}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Questo gioco non è disponibile per le mod.", $language)}</p>
  {/if}
  {#if modding !== null}
    <Panel title={t("Gioco Thunderstore", $language)} description={t("Il catalogo e i profili delle mod dipendono da questo collegamento.", $language)}>
      <div class="flex flex-col gap-3">
        <p class="text-sm text-zinc-200 light:text-zinc-800">{bindingText}</p>
        {#if modState === "unsupported"}
          <Callout tone="warning" variant="inline" message={t("Il loader di questo gioco non è ancora supportato: Legio installa le mod dei giochi con BepInEx.", $language)} />
        {/if}
        <div class="flex flex-wrap gap-2">
          <Button label={t("Scegli gioco Thunderstore...", $language)} variant="secondary" disabled={running} onClick={() => (pickerOpen = true)} />
          {#if modState === "ready"}
            <Button label={t("Gestisci mod", $language)} variant="secondary" onClick={() => openModManager(game.id, "game")} />
          {/if}
        </div>
        {#if running}<p class="text-xs text-zinc-500">{t("Chiudi il gioco per cambiare il collegamento.", $language)}</p>{/if}
      </div>
    </Panel>
    {#if modding.binding !== null}
      <Panel title={t("Cartella del gioco", $language)} description={t("Rimuove i file del loader che Legio ha copiato nella cartella del gioco, ad esempio prima di verificarne i file.", $language)}>
        <div class="flex flex-col gap-3">
          <div>
            <Button label={restoring ? t("Ripristino...", $language) : t("Ripristina cartella del gioco", $language)} variant="secondary" disabled={running || restoring} onClick={() => void restore()} />
          </div>
          {#if restored}<Callout tone="success" variant="inline" message={t("Cartella del gioco ripristinata.", $language)} />{/if}
          {#if restoreError !== null}<ErrorBanner message={restoreError} />{/if}
        </div>
      </Panel>
    {/if}
  {/if}
</div>

{#if modding !== null}
  <EcosystemGameDialog
    open={pickerOpen}
    gameName={game.name}
    currentId={modding.binding?.ecosystemGameId ?? null}
    suggestedId={modding.ecosystemGameId}
    onClose={() => (pickerOpen = false)}
    onSelect={(id) => bindGame(game.id, id)}
  />
{/if}
