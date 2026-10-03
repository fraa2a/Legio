<script lang="ts">
  import { t, language } from "../../i18n";
  import { untrack } from "svelte";
  import type { Game } from "../../services/local-state";
  import { pickExecutableFile } from "../../services/dialog";
  import { toMessage } from "../../utils/errors";
  import { saveExecutableForGame } from "../../stores/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";

  let { game }: { game: Game } = $props();

  let executablePath = $state(untrack(() => game.executablePath ?? ""));
  let pending = $state(false);
  let actionError = $state<string | null>(null);

  async function browse(): Promise<void> {
    actionError = null;
    try {
      executablePath = (await pickExecutableFile(game.executablePath)) ?? executablePath;
    } catch (error) {
      actionError = toMessage(error);
    }
  }

  async function save(): Promise<void> {
    actionError = null;
    pending = true;
    try {
      const updated = await saveExecutableForGame(game.id, executablePath.trim());
      executablePath = updated.executablePath ?? "";
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }
</script>

<Panel title={t("Eseguibile", $language)}>
  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}
  <TextField id="game-executable" label={t("Percorso eseguibile (.exe)", $language)} value={executablePath} disabled={pending} placeholder={t("Seleziona o inserisci un file .exe", $language)} oninput={(value) => (executablePath = value)} />
  <div class="flex flex-wrap gap-2">
    <Button label={t("Scegli eseguibile...", $language)} variant="secondary" disabled={pending} onClick={() => void browse()} />
    <Button label={pending ? t("Salvataggio...", $language) : t("Salva eseguibile", $language)} disabled={pending || executablePath.trim().length === 0 || executablePath.trim() === game.executablePath} onClick={() => void save()} />
  </div>
</Panel>
