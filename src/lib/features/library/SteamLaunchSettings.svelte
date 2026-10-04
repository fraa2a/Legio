<script lang="ts">
  import { t, language } from "../../i18n";
  import { onDestroy, onMount } from "svelte";
  import type { Game } from "../../services/local-state";
  import { getSteamLaunchConfig, saveSteamLaunchConfig } from "../../services/game-settings";
  import { toMessage } from "../../utils/errors";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import { formatLaunchArguments, parseLaunchArguments } from "../../utils/launch-arguments";

  let { game }: { game: Game } = $props();
  let argumentsText = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);
  let baseline = $state<string | null>(null);
  let failedValue: string | null = null;
  let savingTask: Promise<void> | null = null;

  onMount(() => {
    void getSteamLaunchConfig(game.id)
      .then((config) => { argumentsText = formatLaunchArguments(config.arguments); baseline = argumentsText; })
      .catch((cause) => (error = toMessage(cause)))
      .finally(() => (loading = false));
  });

  $effect(() => {
    if (loading || saving || baseline === null || argumentsText === baseline || argumentsText === failedValue) return;
    const timer = setTimeout(() => void save(), 550);
    return () => clearTimeout(timer);
  });

  onDestroy(() => { if (!loading && !saving && baseline !== null && argumentsText !== baseline && argumentsText !== failedValue) void save(); });

  function save(): Promise<void> {
    if (savingTask !== null) return savingTask;
    const value = argumentsText;
    savingTask = (async () => {
      saving = true;
      error = null;
      saved = false;
      try {
        await saveSteamLaunchConfig(game.id, { arguments: parseLaunchArguments(value) });
        baseline = value;
        failedValue = null;
        saved = true;
      } catch (cause) {
        failedValue = value;
        error = toMessage(cause);
      } finally {
        saving = false;
        savingTask = null;
      }
    })();
    return savingTask;
  }
</script>

<Panel title={t("Argomenti di avvio Steam", $language)}>
  {#if error !== null}<ErrorBanner message={error} />{/if}
  <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><TextField
    id="steam-launch-arguments"
    label={t("Opzioni di avvio", $language)}
    value={argumentsText}
    placeholder="-windowed -novid"
    hint={t("Aggiungi argomenti oppure racchiudi un valore con spazi tra virgolette.", $language)}
    disabled={loading || saving}
    oninput={(value) => (argumentsText = value)}
  /></div><ResetSetting label={t("Ripristina opzioni di avvio", $language)} disabled={loading || saving || argumentsText === ""} onClick={() => { argumentsText = ""; }} /></div>
  <p class="text-xs text-zinc-500">{t("Gli argomenti vengono passati a Steam quando avvii questo gioco da Legio.", $language)}</p>
  <div class="flex items-center gap-3">
    {#if saving}<span class="text-sm text-zinc-400" role="status">{t("Salvataggio...", $language)}</span>{/if}
    {#if saved}<span class="text-sm text-emerald-400" role="status">{t("Salvato", $language)}</span>{/if}
  </div>
</Panel>
