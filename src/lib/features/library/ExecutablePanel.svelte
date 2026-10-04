<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy, untrack } from "svelte";
  import type { Game } from "../../services/local-state";
  import { pickExecutableFile } from "../../services/dialog";
  import { toMessage } from "../../utils/errors";
  import { saveExecutableForGame } from "../../stores/manual-import";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";

  let { game }: { game: Game } = $props();

  let executablePath = $state(untrack(() => game.executablePath ?? ""));
  let pending = $state(false);
  let actionError = $state<string | null>(null);
  let failedPath: string | null = null;

  $effect(() => {
    const value = executablePath.trim();
    if (pending || value.length === 0 || value === game.executablePath || value === failedPath) return;
    const timer = setTimeout(() => void save(), 550);
    return () => clearTimeout(timer);
  });

  onDestroy(() => { if (!pending && executablePath.trim().length > 0 && executablePath.trim() !== game.executablePath && executablePath.trim() !== failedPath) void save(); });

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
      failedPath = null;
    } catch (error) {
      failedPath = executablePath.trim();
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
  <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><TextField id="game-executable" label={t("Percorso eseguibile (.exe)", $language)} value={executablePath} disabled={pending} placeholder={t("Seleziona o inserisci un file .exe", $language)} oninput={(value) => (executablePath = value)} /></div><ResetSetting label={t("Ripristina percorso eseguibile", $language)} disabled={pending || executablePath === (game.executablePath ?? "")} onClick={() => { executablePath = game.executablePath ?? ""; }} /></div>
  <div class="flex flex-wrap gap-2">
    <Button label={t("Scegli eseguibile...", $language)} variant="secondary" disabled={pending} onClick={() => void browse()} />
    {#if pending}<span class="text-sm text-zinc-400" role="status">{t("Salvataggio...", $language)}</span>{/if}
  </div>
</Panel>
