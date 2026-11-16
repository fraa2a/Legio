<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import type { Game } from "../../services/local-state";
  import { buildModSupportReport, openModLog, readModLog } from "../../services/modding";
  import { launchStateByGame } from "../../stores/launch";
  import { bindGame, loadModdableGames, moddableGameById, moddableGames, restoreGameFolder } from "../../stores/modding";
  import { openModManager } from "../../stores/navigation";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import EcosystemGameDialog from "./EcosystemGameDialog.svelte";
  import { canChooseEcosystemGame, describeModdingError, moddableGameState } from "./modding-model";
  import { describeToolsError } from "./mod-tools-model";

  let { game }: { game: Game } = $props();

  onMount(() => void loadModdableGames());

  const modding = $derived($moddableGameById.get(game.id) ?? null);
  const modState = $derived(modding === null ? null : moddableGameState(modding));
  const running = $derived(($launchStateByGame.get(game.id)?.status ?? "idle") !== "idle");
  const canChoose = $derived(modding !== null && canChooseEcosystemGame(modding));

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

  let diagnosing = $state(false);
  let diagnosticsError = $state<string | null>(null);

  async function diagnose(work: () => Promise<void>): Promise<void> {
    diagnosing = true;
    diagnosticsError = null;
    try {
      await work();
    } catch (error) {
      diagnosticsError = describeToolsError(toMessage(error), $language);
    } finally {
      diagnosing = false;
    }
  }

  async function copyText(load: () => Promise<string>, done: string): Promise<void> {
    const text = await load();
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      throw new Error(t("Impossibile copiare negli appunti.", $language));
    }
    showToast(done, "success");
  }

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
    <p class="text-sm text-legio-fg-muted">{t("Questo gioco non è disponibile per le mod.", $language)}</p>
  {/if}
  {#if modding !== null}
    <Panel title={t("Gioco Thunderstore", $language)} description={t("Il catalogo e i profili delle mod dipendono da questo collegamento.", $language)}>
      <div class="flex flex-col gap-3">
        <p class="text-sm text-legio-fg">{bindingText}</p>
        {#if modState === "unsupported"}
          <Callout tone="warning" variant="inline" message={t("Il loader di questo gioco non è ancora supportato: Legio installa le mod dei giochi con BepInEx.", $language)} />
        {/if}
        {#if canChoose || modState === "ready"}
          <div class="flex flex-wrap gap-2">
            {#if canChoose}
              <Button label={t("Scegli gioco Thunderstore...", $language)} variant="secondary" disabled={running} onClick={() => (pickerOpen = true)} />
            {/if}
            {#if modState === "ready"}
              <Button label={t("Gestisci mod", $language)} variant="secondary" onClick={() => openModManager(game.id, "game")} />
            {/if}
          </div>
        {/if}
        {#if canChoose && running}<p class="text-xs text-legio-fg-subtle">{t("Chiudi il gioco per cambiare il collegamento.", $language)}</p>{/if}
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
      <Panel title={t("Diagnostica", $language)} description={t("Copia il log di BepInEx o le informazioni sul setup da condividere quando chiedi supporto. La cartella utente viene sostituita da ~.", $language)}>
        <div class="flex flex-col gap-3">
          <div class="flex flex-wrap gap-2">
            <Button label={t("Copia log", $language)} variant="secondary" disabled={diagnosing} onClick={() => void diagnose(() => copyText(() => readModLog(game.id), t("Log copiato negli appunti.", $language)))} />
            <Button label={t("Apri log", $language)} variant="secondary" disabled={diagnosing} onClick={() => void diagnose(() => openModLog(game.id))} />
            <Button label={t("Copia info per supporto", $language)} variant="secondary" disabled={diagnosing} onClick={() => void diagnose(() => copyText(() => buildModSupportReport(game.id), t("Informazioni copiate negli appunti.", $language)))} />
          </div>
          {#if diagnosticsError !== null}<ErrorBanner message={diagnosticsError} />{/if}
        </div>
      </Panel>
    {/if}
  {/if}
</div>

{#if canChoose && modding !== null}
  <EcosystemGameDialog
    open={pickerOpen}
    gameName={game.name}
    currentId={modding.binding?.ecosystemGameId ?? null}
    suggestedId={modding.ecosystemGameId}
    onClose={() => (pickerOpen = false)}
    onSelect={(id) => bindGame(game.id, id)}
  />
{/if}
