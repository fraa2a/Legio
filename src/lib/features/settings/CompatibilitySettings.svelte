<script lang="ts">
  import { mapToText, parseMap, parseEnvironment } from "../../services/launch-fields";
  import { isGraphicsRenderer, isWaylandMode } from "../../services/game-settings";
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import TextField from "../../components/ui/TextField.svelte";
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

  onMount(() => {
    void load();
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

  async function save(): Promise<void> {
    saving = true;
    saveError = null;
    saved = false;
    try {
      defaults = await saveCompatibilityDefaults({
        ...defaults,
        workingDirectory: null,
        argumentsBefore: parseArguments(argumentsBefore),
        argumentsAfter: parseArguments(argumentsAfter),
        environment: parseEnvironment(environmentText, defaults.debugLogging),
        dllOverrides: parseMap(dllOverridesText, t("Override DLL", $language)),
      });
      argumentsBefore = defaults.argumentsBefore.join("\n");
      argumentsAfter = defaults.argumentsAfter.join("\n");
      environmentText = mapToText(defaults.environment);
      dllOverridesText = mapToText(defaults.dllOverrides);
      saved = true;
    } catch (error) {
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
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

    <SettingsGroup title="Runner">
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
      <div class="max-w-sm">
        <SelectField
          id="compat-runner"
          label={t("Runner predefinito", $language)}
          value={defaults.runnerPath ?? ""}
          options={runnerOptions}
          onChange={(value) => (defaults = { ...defaults, runnerPath: value || null })}
        />
      </div>
      {#if logsDirectory !== null}
        <p class="break-all text-xs text-zinc-500 light:text-zinc-600">{t("Log compatibilità: ", $language)}{logsDirectory}</p>
      {/if}
    </SettingsGroup>

    <SettingsGroup title={t("Configurazione di avvio", $language)}>
      <div class="grid gap-4 md:grid-cols-2">
        <TextField
          id="compat-prefix-root"
          label={t("Cartella predefinita dei prefix", $language)}
          value={defaults.prefixRoot ?? ""}
          placeholder={t("Percorso opzionale", $language)}
          hint={t("Legio crea un prefix per gioco dentro questa cartella.", $language)}
          oninput={(value) => (defaults = { ...defaults, prefixRoot: value || null })}
        />
        <label class="flex items-center gap-2 self-end pb-2 text-sm text-zinc-300 light:text-zinc-700">
          <input
            type="checkbox"
            class="size-4 accent-white"
            checked={defaults.debugLogging}
            onchange={(event) => (defaults = { ...defaults, debugLogging: event.currentTarget.checked })}
          />{t("\n          Abilita log di debug per gli avvii\n        ", $language)}</label>
      </div>
      <div class="grid gap-4 md:grid-cols-2">
        <SelectField
          id="compat-renderer"
          label={t("Renderer grafico", $language)}
          value={defaults.graphicsRenderer}
          options={[{ value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "wine_d3d", label: "WineD3D" }]}
          onChange={setGraphicsRenderer}
        />
        <SelectField
          id="compat-wayland"
          label="Wayland"
          value={defaults.wayland}
          options={[{ value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "disabled", label: t("Disattivato", $language) }, { value: "native", label: t("Nativo, solo GE-Proton", $language) }]}
          onChange={setWayland}
        />
      </div>
    </SettingsGroup>

    <SettingsGroup title={t("Argomenti e ambiente", $language)}>
      <div class="grid gap-4 md:grid-cols-2">
        <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Argomenti prima dell'eseguibile\n          ", $language)}<textarea bind:value={argumentsBefore} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Un argomento per riga. Le righe vuote rappresentano argomenti vuoti.", $language)}</span>
        </label>
        <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Argomenti dopo l'eseguibile\n          ", $language)}<textarea bind:value={argumentsAfter} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Un argomento per riga, passato senza interpretazione shell.", $language)}</span>
        </label>
        <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Variabili ambiente\n          ", $language)}<textarea bind:value={environmentText} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga. I controlli tipizzati hanno priorità.", $language)}</span>
        </label>
        <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n          Override DLL\n          ", $language)}<textarea bind:value={dllOverridesText} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
          <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.", $language)}</span>
        </label>
      </div>
    </SettingsGroup>

    <div class="flex flex-wrap items-center gap-3">
      {#if saveError !== null}
        <ErrorBanner message={saveError} />
      {/if}
      {#if saved}
        <p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Default salvati.", $language)}</p>
      {/if}
      <Button label={saving ? t("Salvataggio...", $language) : t("Salva default", $language)} disabled={saving || loading} onClick={() => void save()} />
    </div>
  {/if}
</section>