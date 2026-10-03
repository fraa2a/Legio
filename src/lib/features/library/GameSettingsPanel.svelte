<script lang="ts">
  import { mapToText, parseMap, parseEnvironment } from "../../services/launch-fields";
  import { isGraphicsRenderer, isWaylandMode } from "../../services/game-settings";
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import type { Game } from "../../services/local-state";
  import {
    emptyGameCompatibilityOverrides,
    getCompatibilityDefaults,
    getGameCompatibilityOverrides,
    getGameOnlineFixDetected,
    getNativeLaunchConfig,
    listCompatibilityRunners,
    saveGameCompatibilityOverrides,
    saveNativeLaunchConfig,
    type CompatibilityDefaults,
    type GameCompatibilityOverrides,
    type NativeLaunchConfig,
  } from "../../services/game-settings";
  import { appInfo } from "../../stores/app-info";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { toMessage } from "../../utils/errors";
  import { pickGameDirectory } from "../../services/dialog";
  import { formatLaunchArguments, parseLaunchArguments } from "../../utils/launch-arguments";

  let { game, section }: { game: Game; section: "locations" | "launch" | "compatibility" } = $props();

  let overrides = $state<GameCompatibilityOverrides>({ ...emptyGameCompatibilityOverrides });
  let onlineFixDetected = $state(false);
  let defaults = $state<CompatibilityDefaults | null>(null);
  let runners = $state<{ kind: string; name: string; version: string; path: string }[]>([]);
  let runnerDiagnostics = $state<string[]>([]);
  let argumentsBefore = $state("");
  let argumentsAfter = $state("");
  let environmentText = $state("");
  let dllOverridesText = $state("");
  let nativeConfig = $state<NativeLaunchConfig>({ arguments: [], workingDirectory: null });
  let nativeArguments = $state("");
  let nativeWorkingDirectory = $state("");
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
    saveError = null;
    if ($appInfo.status === "idle" || $appInfo.status === "loading") await appInfo.load();
    if ($appInfo.status === "error") {
      loadError = $appInfo.error;
      loading = false;
      return;
    }
    const platform = $appInfo.data.platform;
    if (platform === "linux") {
      const [overrideResult, defaultsResult, runnersResult, onlineFixResult] = await Promise.allSettled([
        getGameCompatibilityOverrides(game.id),
        getCompatibilityDefaults(),
        listCompatibilityRunners(),
        section === "compatibility" ? getGameOnlineFixDetected(game.id) : Promise.resolve(false),
      ]);
      if (overrideResult.status === "fulfilled") {
        overrides = overrideResult.value;
        argumentsBefore = formatLaunchArguments(overrides.argumentsBefore ?? []);
        argumentsAfter = formatLaunchArguments(overrides.argumentsAfter ?? []);
        environmentText = mapToText(overrides.environment ?? {});
        dllOverridesText = mapToText(overrides.dllOverrides ?? {});
      } else {
        loadError = toMessage(overrideResult.reason);
      }
      if (defaultsResult.status === "fulfilled") defaults = defaultsResult.value;
      else if (loadError === null) loadError = toMessage(defaultsResult.reason);
      if (runnersResult.status === "fulfilled") {
        runners = runnersResult.value.runners;
        runnerDiagnostics = runnersResult.value.diagnostics;
      } else if (loadError === null) {
        loadError = toMessage(runnersResult.reason);
      }
      if (onlineFixResult.status === "fulfilled") onlineFixDetected = onlineFixResult.value;
      else if (loadError === null) loadError = toMessage(onlineFixResult.reason);
    } else if (platform === "windows") {
      try {
        nativeConfig = await getNativeLaunchConfig(game.id);
        nativeArguments = formatLaunchArguments(nativeConfig.arguments);
        nativeWorkingDirectory = nativeConfig.workingDirectory ?? "";
      } catch (error) {
        loadError = toMessage(error);
      }
    }
    loading = false;
  }



  function setOverride<K extends keyof GameCompatibilityOverrides>(
    key: K,
    value: GameCompatibilityOverrides[K],
  ): void {
    overrides = { ...overrides, [key]: value };
  }

  function togglePrefix(enabled: boolean): void {
    setOverride("prefixPath", enabled ? "" : null);
  }

  async function browsePrefix(): Promise<void> {
    try {
      const path = await pickGameDirectory(overrides.prefixPath, t("Seleziona la cartella del prefix", $language));
      if (path !== null) setOverride("prefixPath", path);
    } catch (error) {
      saveError = toMessage(error);
    }
  }

  function toggleWorkingDirectory(enabled: boolean): void {
    setOverride("workingDirectory", enabled ? "" : null);
  }

  function toggleArgumentsBefore(enabled: boolean): void {
    const value = enabled ? [...(defaults?.argumentsBefore ?? [])] : null;
    setOverride("argumentsBefore", value);
    argumentsBefore = formatLaunchArguments(value ?? []);
  }

  function toggleArgumentsAfter(enabled: boolean): void {
    const value = enabled ? [...(defaults?.argumentsAfter ?? [])] : null;
    setOverride("argumentsAfter", value);
    argumentsAfter = formatLaunchArguments(value ?? []);
  }

  function toggleEnvironment(enabled: boolean): void {
    const value = enabled ? { ...(defaults?.environment ?? {}) } : null;
    setOverride("environment", value);
    environmentText = mapToText(value ?? {});
  }

  function toggleDllOverrides(enabled: boolean): void {
    const value = enabled ? { ...(defaults?.dllOverrides ?? {}) } : null;
    setOverride("dllOverrides", value);
    dllOverridesText = mapToText(value ?? {});
  }

  function setGraphicsRenderer(value: string): void {
    if (value === "inherit" || isGraphicsRenderer(value)) setOverride("graphicsRenderer", value === "inherit" ? null : value);
  }

  function setWayland(value: string): void {
    if (value === "inherit" || isWaylandMode(value)) setOverride("wayland", value === "inherit" ? null : value);
  }

  function setDebugLogging(value: string): void {
    setOverride("debugLogging", value === "inherit" ? null : value === "true");
  }

  const selectedRunnerPath = $derived(overrides.runnerPath ?? defaults?.runnerPath ?? runners[0]?.path ?? "");
  const runnerChoices = $derived.by(() => {
    if (selectedRunnerPath.length > 0 && !runners.some((runner) => runner.path === selectedRunnerPath)) {
      return [{ kind: "configured", name: selectedRunnerPath, version: "", path: selectedRunnerPath }, ...runners];
    }
    return runners;
  });

  function selectRunner(path: string): void {
    const inheritedRunner = defaults?.runnerPath ?? runners[0]?.path ?? "";
    setOverride("runnerPath", path === inheritedRunner ? null : path);
  }

  async function saveCompatibility(): Promise<void> {
    saving = true;
    saveError = null;
    saved = false;
    try {
      if (overrides.prefixPath !== null && overrides.prefixPath.trim().length === 0) {
        throw new Error(t("Seleziona una cartella per il prefix personalizzato.", $language));
      }
      overrides = await saveGameCompatibilityOverrides(game.id, {
        ...overrides,
        argumentsBefore: overrides.argumentsBefore === null ? null : parseLaunchArguments(argumentsBefore),
        argumentsAfter: overrides.argumentsAfter === null ? null : parseLaunchArguments(argumentsAfter),
        environment: overrides.environment === null ? null : parseEnvironment(environmentText, overrides.debugLogging ?? defaults?.debugLogging ?? false),
        dllOverrides: overrides.dllOverrides === null ? null : parseMap(dllOverridesText, t("Override DLL", $language)),
      });
      saved = true;
    } catch (error) {
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
  }

  async function resetCompatibility(): Promise<void> {
    saving = true;
    saveError = null;
    saved = false;
    try {
      overrides = await saveGameCompatibilityOverrides(game.id, { ...emptyGameCompatibilityOverrides });
      argumentsBefore = "";
      argumentsAfter = "";
      environmentText = "";
      dllOverridesText = "";
      saved = true;
    } catch (error) {
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
  }

  async function saveNative(): Promise<void> {
    saving = true;
    saveError = null;
    saved = false;
    try {
      nativeConfig = await saveNativeLaunchConfig(game.id, {
        arguments: parseLaunchArguments(nativeArguments),
        workingDirectory: nativeWorkingDirectory || null,
      });
      nativeArguments = formatLaunchArguments(nativeConfig.arguments);
      nativeWorkingDirectory = nativeConfig.workingDirectory ?? "";
      saved = true;
    } catch (error) {
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
  }
</script>

<Panel title={section === "locations" ? t("Percorsi del gioco", $language) : section === "launch" ? "Launch Options" : t("Compatibilità", $language)}>
  {#if loading}
    <p class="text-sm text-zinc-400" role="status">{t("Caricamento delle impostazioni di avvio...", $language)}</p>
  {:else if loadError !== null}
    <ErrorBanner message={loadError} onRetry={() => void load()} />
  {:else if $appInfo.data.platform === "linux" && section === "locations"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Il prefix predefinito è una cartella distinta per questo gioco sotto la radice globale. Avvio con Steam usa lo stesso percorso.", $language)}</p>
    <div class="grid gap-4">
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <span class="flex items-center gap-2">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.prefixPath !== null} onchange={(event) => togglePrefix(event.currentTarget.checked)} />{t("\n          Percorso del prefix personalizzato\n        ", $language)}</span>
        <div class="flex gap-2">
          <div class="min-w-0 flex-1"><TextField id="game-compat-prefix" label={t("Cartella del prefix", $language)} value={overrides.prefixPath ?? ""} disabled={overrides.prefixPath === null} placeholder={t("Percorso assoluto", $language)} oninput={(value) => setOverride("prefixPath", value)} /></div>
          <Button label={t("Sfoglia...", $language)} variant="secondary" disabled={overrides.prefixPath === null} onClick={() => void browsePrefix()} />
        </div>
      </div>
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <span class="flex items-center gap-2">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.workingDirectory !== null} onchange={(event) => toggleWorkingDirectory(event.currentTarget.checked)} />{t("\n          Cartella di lavoro personalizzata\n        ", $language)}</span>
        <TextField id="game-compat-working-directory" label={t("Cartella di lavoro", $language)} value={overrides.workingDirectory ?? ""} disabled={overrides.workingDirectory === null} placeholder={t("Predefinita: cartella dell'eseguibile", $language)} oninput={(value) => setOverride("workingDirectory", value)} />
      </div>
    </div>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Percorsi salvati.", $language)}</p>{/if}
    <div><Button label={saving ? t("Salvataggio...", $language) : t("Salva percorsi", $language)} disabled={saving} onClick={() => void saveCompatibility()} /></div>
  {:else if $appInfo.data.platform === "linux" && section === "compatibility"}
    {#if game.steamAppId !== null}
      <label class="flex items-center gap-2 text-sm text-zinc-200 light:text-zinc-800">
        <input type="checkbox" class="size-4 accent-white" checked={overrides.launchViaSteam ?? true}
          onchange={(event) => setOverride("launchViaSteam", event.currentTarget.checked)} />{t("\n        Avvio con Steam\n      ", $language)}</label>
      <p class="text-xs text-zinc-500">{t("Avvia Steam e il gioco con Proton usando il prefix della sezione Posizioni. Runtime e overlay vengono applicati automaticamente quando disponibili.", $language)}</p>
    {/if}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n      Ogni campo eredita il default globale finché il relativo override resta disattivato. Una lista o una mappa vuota cancella il valore ereditato.\n    ", $language)}</p>
    {#if runnerDiagnostics.length > 0}
      <div class="rounded-lg bg-amber-500/10 p-3 text-sm text-amber-200 light:text-amber-900">
        {#each runnerDiagnostics as diagnostic (diagnostic)}<p>{diagnostic}</p>{/each}
      </div>
    {/if}
    <div class="flex flex-col gap-2">
      <h3 class="text-lg font-semibold text-zinc-100 light:text-zinc-900">Proton Version</h3>
      {#if runnerChoices.length === 0}
        <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Nessun runner compatibile installato.", $language)}</p>
      {:else}
        <fieldset class="flex flex-col gap-1">
          <legend class="sr-only">{t("Runner di compatibilità", $language)}</legend>
          {#each runnerChoices as runner (runner.path)}
            <label class="flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-sm text-zinc-200 transition-colors hover:bg-white/5 has-checked:bg-white/10 light:text-zinc-800 light:hover:bg-zinc-900/5 light:has-checked:bg-zinc-900/10">
              <input type="radio" name="game-compat-runner" value={runner.path} checked={selectedRunnerPath === runner.path} onchange={() => selectRunner(runner.path)} class="size-4 accent-white" />
              <span class="min-w-0 truncate">{runner.name}{runner.kind === "wine" && runner.version ? ` ${runner.version}` : ""}</span>
              {#if runner.path === (defaults?.runnerPath ?? runners[0]?.path)}<span class="ml-auto shrink-0 text-xs text-zinc-500">Default</span>{/if}
            </label>
          {/each}
        </fieldset>
      {/if}
    </div>
    <div class="grid gap-4 md:grid-cols-2">
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n        Variabili ambiente\n        ", $language)}<span class="flex items-center gap-2 text-xs">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.environment !== null} onchange={(event) => toggleEnvironment(event.currentTarget.checked)} />{t("\n          Personalizza le variabili\n        ", $language)}</span>
        <textarea bind:value={environmentText} disabled={overrides.environment === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga. I controlli tipizzati gestiscono le variabili riservate.", $language)}</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("\n        Override DLL\n        ", $language)}<span class="flex items-center gap-2 text-xs">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.dllOverrides !== null} onchange={(event) => toggleDllOverrides(event.currentTarget.checked)} />{t("\n          Personalizza gli override\n        ", $language)}</span>
        <textarea bind:value={dllOverridesText} disabled={overrides.dllOverrides === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.", $language)}</span>
      </label>
    </div>

    <label class="flex items-center gap-2 text-sm text-zinc-200 light:text-zinc-800">
      <input type="checkbox" class="size-4 accent-white"
        checked={overrides.onlineFix ?? onlineFixDetected}
        onchange={(event) => setOverride("onlineFix", event.currentTarget.checked ? (onlineFixDetected ? null : true) : false)} />{t("\n      Avvia con OnlineFix\n    ", $language)}</label>
    {#if onlineFixDetected}<p class="text-xs text-zinc-500">{t("OnlineFix64.dll rilevato nella cartella del gioco.", $language)}</p>{/if}

    <div class="grid gap-4 md:grid-cols-2">
      <SelectField id="game-compat-renderer" label={t("Renderer grafico", $language)} value={overrides.graphicsRenderer ?? "inherit"} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "wine_d3d", label: "WineD3D" }]} onChange={setGraphicsRenderer} />
      <SelectField id="game-compat-wayland" label="Wayland" value={overrides.wayland ?? "inherit"} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "disabled", label: t("Disattivato", $language) }, { value: "native", label: t("Nativo, solo GE-Proton", $language) }]} onChange={setWayland} />
      <SelectField id="game-compat-debug" label={t("Log di debug", $language)} value={overrides.debugLogging === null ? "inherit" : String(overrides.debugLogging)} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "true", label: t("Attivi", $language) }, { value: "false", label: t("Disattivi", $language) }]} onChange={setDebugLogging} />
    </div>

    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Impostazioni salvate.", $language)}</p>{/if}
    <div class="flex flex-wrap gap-2">
      <Button label={saving ? t("Salvataggio...", $language) : t("Salva impostazioni", $language)} disabled={saving} onClick={() => void saveCompatibility()} />
      <Button label={t("Ripristina default globali", $language)} variant="secondary" disabled={saving} onClick={() => void resetCompatibility()} />
    </div>
  {:else if $appInfo.data.platform === "linux"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.", $language)}</p>
    <div class="grid gap-4 md:grid-cols-2">
      <div class="flex flex-col gap-2">
        <label class="flex items-center gap-2 text-sm text-zinc-300 light:text-zinc-700">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.argumentsBefore !== null} onchange={(event) => toggleArgumentsBefore(event.currentTarget.checked)} />{t("\n          Prima dell'eseguibile\n        ", $language)}</label>
        <TextField id="arguments-before" label="Launch Options" value={argumentsBefore} disabled={overrides.argumentsBefore === null} placeholder="-windowed -novid" oninput={(value) => (argumentsBefore = value)} />
      </div>
      <div class="flex flex-col gap-2">
        <label class="flex items-center gap-2 text-sm text-zinc-300 light:text-zinc-700">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.argumentsAfter !== null} onchange={(event) => toggleArgumentsAfter(event.currentTarget.checked)} />{t("\n          Dopo l'eseguibile\n        ", $language)}</label>
        <TextField id="arguments-after" label="Launch Options" value={argumentsAfter} disabled={overrides.argumentsAfter === null} placeholder="-windowed -novid" oninput={(value) => (argumentsAfter = value)} />
      </div>
    </div>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Argomenti salvati.", $language)}</p>{/if}
    <div><Button label={saving ? t("Salvataggio...", $language) : t("Salva opzioni", $language)} disabled={saving} onClick={() => void saveCompatibility()} /></div>
  {:else if $appInfo.data.platform === "windows" && section === "launch"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.", $language)}</p>
    <TextField id="native-arguments" label="Launch Options" value={nativeArguments} placeholder="-windowed -novid" oninput={(value) => (nativeArguments = value)} />
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Argomenti salvati.", $language)}</p>{/if}
    <div><Button label={saving ? t("Salvataggio...", $language) : t("Salva opzioni", $language)} disabled={saving} onClick={() => void saveNative()} /></div>
  {:else if $appInfo.data.platform === "windows"}
    {#if section === "locations"}
      <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Imposta la cartella iniziale del processo per questo gioco.", $language)}</p>
      <TextField id="native-working-directory" label={t("Cartella di lavoro", $language)} value={nativeWorkingDirectory} placeholder={t("Cartella dell'eseguibile", $language)} oninput={(value) => (nativeWorkingDirectory = value)} />
      {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
      {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Percorso salvato.", $language)}</p>{/if}
      <div><Button label={saving ? t("Salvataggio...", $language) : t("Salva percorso", $language)} disabled={saving} onClick={() => void saveNative()} /></div>
    {:else}
      <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Le opzioni di compatibilità aggiuntive non sono disponibili per i runner nativi. Configura gli argomenti di avvio in Generali.", $language)}</p>
    {/if}
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Le impostazioni di avvio manuale non sono disponibili su questa piattaforma.", $language)}</p>
  {/if}
</Panel>
