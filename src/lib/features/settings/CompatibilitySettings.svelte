<script lang="ts">
  import { mapToText, parseMap, parseEnvironment } from "../../services/launch-fields";
  import { isGraphicsRenderer, isWaylandMode } from "../../services/game-settings";
  import { t, language } from "../../i18n";
  import { onDestroy, onMount } from "svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import {
    emptyCompatibilityDefaults,
    getCompatibilityDefaults,
    getCompatibilityLogsDirectory,
    listCompatibilityRunners,
    saveCompatibilityDefaults,
    type CompatibilityDefaults,
  } from "../../services/game-settings";
  import { appInfo } from "../../stores/app-info";
  import { toMessage } from "../../utils/errors";

  let defaults = $state<CompatibilityDefaults>({ ...emptyCompatibilityDefaults });
  let argumentsBefore = $state("");
  let argumentsAfter = $state("");
  let environmentText = $state("");
  let dllOverridesText = $state("");
  let runners = $state<{ kind: string; name: string; version: string; path: string }[]>([]);
  let runnerDiagnostics = $state<string[]>([]);
  let logsDirectory = $state<string | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let loadError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let saved = $state(false);
  let baseline = $state<string | null>(null);
  let failedSnapshot: string | null = null;
  let savingTask: Promise<void> | null = null;
  const serialized = $derived(JSON.stringify({ defaults, argumentsBefore, argumentsAfter, environmentText, dllOverridesText }));

  onMount(() => {
    void load();
  });

  $effect(() => {
    if (baseline === null || serialized === baseline || serialized === failedSnapshot || saving) return;
    const timer = setTimeout(() => void save(), 550);
    return () => clearTimeout(timer);
  });

  onDestroy(() => {
    void (async () => {
      if (savingTask !== null) await savingTask;
      if (baseline !== null && serialized !== baseline && serialized !== failedSnapshot) await save();
    })();
  });

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    if ($appInfo.status === "idle" || $appInfo.status === "loading") await appInfo.load();
    if ($appInfo.status === "error") {
      loadError = $appInfo.error;
      loading = false;
      return;
    }
    if ($appInfo.data.platform !== "linux") {
      loading = false;
      return;
    }
    const [defaultsResult, runnersResult, logsResult] = await Promise.allSettled([
      getCompatibilityDefaults(),
      listCompatibilityRunners(),
      getCompatibilityLogsDirectory(),
    ]);
    if (defaultsResult.status === "fulfilled") {
      defaults = defaultsResult.value;
      argumentsBefore = defaults.argumentsBefore.join("\n");
      argumentsAfter = defaults.argumentsAfter.join("\n");
      environmentText = mapToText(defaults.environment);
      dllOverridesText = mapToText(defaults.dllOverrides);
      baseline = JSON.stringify({ defaults, argumentsBefore, argumentsAfter, environmentText, dllOverridesText });
    } else {
      loadError = toMessage(defaultsResult.reason);
    }
    if (runnersResult.status === "fulfilled") {
      runners = runnersResult.value.runners;
      runnerDiagnostics = runnersResult.value.diagnostics;
    } else if (loadError === null) {
      loadError = toMessage(runnersResult.reason);
    }
    if (logsResult.status === "fulfilled") logsDirectory = logsResult.value;
    loading = false;
  }

  function parseArguments(value: string): string[] {
    return value === "" ? [] : value.split("\n").map((line) => line.replace(/\r$/, ""));
  }

  function save(): Promise<void> {
    if (savingTask !== null) return savingTask;
    const snapshot = serialized;
    const draft = structuredClone($state.snapshot(defaults));
    const before = argumentsBefore;
    const after = argumentsAfter;
    const environment = environmentText;
    const dll = dllOverridesText;
    savingTask = (async () => {
      saving = true;
      saveError = null;
      saved = false;
      try {
        const updated = await saveCompatibilityDefaults({
          ...draft,
          workingDirectory: null,
          argumentsBefore: parseArguments(before),
          argumentsAfter: parseArguments(after),
          environment: parseEnvironment(environment, draft.debugLogging),
          dllOverrides: parseMap(dll, t("Override DLL", $language)),
        });
        if (serialized === snapshot) {
          defaults = updated;
          argumentsBefore = updated.argumentsBefore.join("\n");
          argumentsAfter = updated.argumentsAfter.join("\n");
          environmentText = mapToText(updated.environment);
          dllOverridesText = mapToText(updated.dllOverrides);
        }
        baseline = JSON.stringify({ defaults: updated, argumentsBefore: updated.argumentsBefore.join("\n"), argumentsAfter: updated.argumentsAfter.join("\n"), environmentText: mapToText(updated.environment), dllOverridesText: mapToText(updated.dllOverrides) });
        failedSnapshot = null;
        saved = true;
      } catch (error) {
        failedSnapshot = snapshot;
        saveError = toMessage(error);
      } finally {
        saving = false;
        savingTask = null;
      }
    })();
    return savingTask;
  }

  const runnerOptions = $derived([
    { value: "", label: t("Scelta automatica", $language) },
    ...(defaults.runnerPath && !runners.some((runner) => runner.path === defaults.runnerPath)
      ? [{ value: defaults.runnerPath, label: defaults.runnerPath }]
      : []),
    ...runners.map((runner) => ({
      value: runner.path,
      label: runner.kind === "wine" ? `${runner.name} (${runner.version})` : runner.name,
    })),
  ]);

  function setGraphicsRenderer(value: string): void {
    if (isGraphicsRenderer(value)) defaults = { ...defaults, graphicsRenderer: value };
  }

  function setWayland(value: string): void {
    if (isWaylandMode(value)) defaults = { ...defaults, wayland: value };
  }
</script>

<section class="flex flex-col gap-4">
  {#if loading}
    <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">{t("Caricamento dei runner e dei default...", $language)}</p>
  {:else if $appInfo.status === "error"}
    <ErrorBanner message={$appInfo.error ?? t("Impossibile rilevare la piattaforma.", $language)} onRetry={() => void load()} />
  {:else if $appInfo.data.platform !== "linux"}
    <p class="rounded-xl bg-white/5 p-4 text-sm text-zinc-400 light:bg-zinc-100 light:text-zinc-600">{t("\n      I runner e i default di compatibilità sono disponibili su Linux. Le impostazioni di avvio native si configurano nella pagina di ogni gioco Windows.\n    ", $language)}</p>
  {:else}
    {#if loadError !== null}
      <ErrorBanner message={loadError} onRetry={() => void load()} />
    {/if}

    <SettingsGroup icon="wrench" title="Runner">
      {#if runnerDiagnostics.length > 0}
        <div class="rounded-lg bg-amber-500/10 p-3 text-sm text-amber-200 light:text-amber-900">
          <p class="font-medium">{t("Rilevamento runner", $language)}</p>
          <ul class="mt-2 list-disc pl-5">
            {#each runnerDiagnostics as diagnostic (diagnostic)}
              <li>{diagnostic}</li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if runners.length === 0}
        <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Nessun runner compatibile rilevato.", $language)}</p>
      {/if}
      <div class="flex max-w-sm items-end gap-2"><div class="min-w-0 flex-1">
        <SelectField
          id="compat-runner"
          label={t("Runner predefinito", $language)}
          value={defaults.runnerPath ?? ""}
          options={runnerOptions}
          onChange={(value) => (defaults = { ...defaults, runnerPath: value || null })}
        />
      </div><ResetSetting label={t("Ripristina runner predefinito", $language)} disabled={saving || defaults.runnerPath === null} onClick={() => { defaults = { ...defaults, runnerPath: null }; }} /></div>
      {#if logsDirectory !== null}
        <p class="break-all text-xs text-zinc-500 light:text-zinc-600">{t("Log compatibilità: ", $language)}{logsDirectory}</p>
      {/if}
    </SettingsGroup>

    <SettingsGroup icon="settings" title={t("Configurazione di avvio", $language)}>
      <div class="grid gap-4 md:grid-cols-2">
        <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><TextField
          id="compat-prefix-root"
          label={t("Cartella predefinita dei prefix", $language)}
          value={defaults.prefixRoot ?? ""}
          placeholder={t("Percorso opzionale", $language)}
          hint={t("Legio crea un prefix per gioco dentro questa cartella.", $language)}
          oninput={(value) => (defaults = { ...defaults, prefixRoot: value || null })}
        /></div><ResetSetting label={t("Ripristina cartella prefix", $language)} disabled={saving || defaults.prefixRoot === null} onClick={() => { defaults = { ...defaults, prefixRoot: null }; }} /></div>
        <div class="flex items-center gap-2 self-end pb-2"><div class="min-w-0 flex-1">
          <Toggle
            label={t("Abilita log di debug per gli avvii", $language)}
            checked={defaults.debugLogging}
            onChange={(checked) => (defaults = { ...defaults, debugLogging: checked })}
          /></div><ResetSetting label={t("Ripristina log di debug", $language)} disabled={saving || !defaults.debugLogging} onClick={() => { defaults = { ...defaults, debugLogging: false }; }} /></div>
      </div>
      <div class="grid gap-4 md:grid-cols-2">
        <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><SelectField
          id="compat-renderer"
          label={t("Renderer grafico", $language)}
          value={defaults.graphicsRenderer}
          options={[{ value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "wine_d3d", label: "WineD3D" }]}
          onChange={setGraphicsRenderer}
        /></div><ResetSetting label={t("Ripristina renderer grafico", $language)} disabled={saving || defaults.graphicsRenderer === "runner_default"} onClick={() => { defaults = { ...defaults, graphicsRenderer: "runner_default" }; }} /></div>
        <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><SelectField
          id="compat-wayland"
          label="Wayland"
          value={defaults.wayland}
          options={[{ value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "disabled", label: t("Disattivato", $language) }, { value: "native", label: t("Nativo, solo GE-Proton", $language) }]}
          onChange={setWayland}
        /></div><ResetSetting label={t("Ripristina Wayland", $language)} disabled={saving || defaults.wayland === "runner_default"} onClick={() => { defaults = { ...defaults, wayland: "runner_default" }; }} /></div>
      </div>
    </SettingsGroup>

    <SettingsGroup icon="info" title={t("Argomenti e ambiente", $language)}>
      <div class="grid gap-4 md:grid-cols-2">
        <div class="relative"><label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Argomenti prima dell'eseguibile\n          ", $language)}<textarea bind:value={argumentsBefore} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Un argomento per riga. Le righe vuote rappresentano argomenti vuoti.", $language)}</span>
        </label><span class="absolute right-0 top-0"><ResetSetting label={t("Ripristina argomenti iniziali", $language)} disabled={saving || argumentsBefore === ""} onClick={() => { argumentsBefore = ""; }} /></span></div>
        <div class="relative"><label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Argomenti dopo l'eseguibile\n          ", $language)}<textarea bind:value={argumentsAfter} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Un argomento per riga, passato senza interpretazione shell.", $language)}</span>
        </label><span class="absolute right-0 top-0"><ResetSetting label={t("Ripristina argomenti finali", $language)} disabled={saving || argumentsAfter === ""} onClick={() => { argumentsAfter = ""; }} /></span></div>
        <div class="relative"><label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Variabili ambiente\n          ", $language)}<textarea bind:value={environmentText} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga. I controlli tipizzati hanno priorità.", $language)}</span>
        </label><span class="absolute right-0 top-0"><ResetSetting label={t("Ripristina variabili ambiente", $language)} disabled={saving || environmentText === ""} onClick={() => { environmentText = ""; }} /></span></div>
        <div class="relative"><label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Override DLL\n          ", $language)}<textarea bind:value={dllOverridesText} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.", $language)}</span>
        </label><span class="absolute right-0 top-0"><ResetSetting label={t("Ripristina override DLL", $language)} disabled={saving || dllOverridesText === ""} onClick={() => { dllOverridesText = ""; }} /></span></div>
      </div>
    </SettingsGroup>

    <div class="flex flex-wrap items-center gap-3">
      {#if saveError !== null}
        <ErrorBanner message={saveError} />
      {/if}
      {#if saved}
        <p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Default salvati.", $language)}</p>
      {/if}
      {#if saving}<p class="text-sm text-zinc-400" role="status">{t("Salvataggio...", $language)}</p>{/if}
    </div>
  {/if}
</section>
