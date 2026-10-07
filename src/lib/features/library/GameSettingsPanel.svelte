<script lang="ts">
  import { mapToText, parseMap, parseEnvironment } from "../../services/launch-fields";
  import { isGraphicsRenderer, isWaylandMode } from "../../services/game-settings";
  import { t, language } from "../../i18n";
  import { onDestroy, onMount } from "svelte";
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
  import Toggle from "../../components/ui/Toggle.svelte";
  import { toMessage } from "../../utils/errors";
  import { pickGameDirectory } from "../../services/dialog";
  import { formatLaunchArguments, parseLaunchArguments } from "../../utils/launch-arguments";

  let { game, section }: { game: Game; section: "locations" | "launch" | "compatibility" } = $props();

  let overrides = $state<GameCompatibilityOverrides>({ ...emptyGameCompatibilityOverrides });
  let onlineFixDetected = $state(false);
  let defaults = $state<CompatibilityDefaults | null>(null);
  let runners = $state<{ kind: string; name: string; version: string; path: string }[]>([]);
  let runnerDiagnostics = $state<string[]>([]);
  let gameModeAvailable = $state(false);
  let gamescopeAvailable = $state(false);
  let argumentsBefore = $state("");
  let argumentsAfter = $state("");
  let environmentText = $state("");
  let dllOverridesText = $state("");
  let nativeConfig = $state<NativeLaunchConfig>({ arguments: [], workingDirectory: null, environment: {} });
  let nativeArguments = $state("");
  let nativeWorkingDirectory = $state("");
  let nativeEnvironmentText = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let loadError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let saved = $state(false);
  let baseline = $state<string | null>(null);
  let failedSnapshot: string | null = null;
  let savingTask: Promise<void> | null = null;
  const compatibilitySnapshot = $derived(JSON.stringify({ overrides, argumentsBefore, argumentsAfter, environmentText, dllOverridesText }));
  const nativeSnapshot = $derived(JSON.stringify({ nativeArguments, nativeWorkingDirectory, nativeEnvironmentText }));
  const currentSnapshot = $derived($appInfo.data.platform === "linux" ? compatibilitySnapshot : nativeSnapshot);

  onMount(() => {
    void load();
  });

  $effect(() => {
    if (loading || loadError !== null || baseline === null || saving || currentSnapshot === baseline || currentSnapshot === failedSnapshot) return;
    const timer = setTimeout(() => void persist(), 550);
    return () => clearTimeout(timer);
  });

  onDestroy(() => {
    void (async () => {
      if (savingTask !== null) await savingTask;
      if (!loading && loadError === null && baseline !== null && currentSnapshot !== baseline && currentSnapshot !== failedSnapshot) await persist();
    })();
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
        section === "compatibility" ? listCompatibilityRunners() : Promise.resolve({ runners: [], diagnostics: [], gameModeAvailable: false, gamescopeAvailable: false }),
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
        gameModeAvailable = runnersResult.value.gameModeAvailable;
        gamescopeAvailable = runnersResult.value.gamescopeAvailable;
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
        nativeEnvironmentText = mapToText(nativeConfig.environment);
      } catch (error) {
        loadError = toMessage(error);
      }
    }
    if (loadError === null) baseline = currentSnapshot;
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

  function setPerformance(changes: Partial<GameCompatibilityOverrides["linuxPerformance"]>): void {
    setOverride("linuxPerformance", { ...overrides.linuxPerformance, ...changes });
  }

  function setGamescopeResolution(value: string): void {
    const config = overrides.linuxPerformance.gamescope;
    if (config === null) return;
    if (value === "native") {
      setPerformance({ gamescope: { ...config, width: null, height: null } });
    } else {
      const [width, height] = value.split("x").map(Number);
      if (Number.isInteger(width) && Number.isInteger(height)) setPerformance({ gamescope: { ...config, width, height } });
    }
  }

  function setGamescopeFps(value: string): void {
    const config = overrides.linuxPerformance.gamescope;
    if (config === null) return;
    const fps = value === "uncapped" ? null : Number(value);
    if (fps === null || (Number.isInteger(fps) && fps >= 1 && fps <= 360)) setPerformance({ gamescope: { ...config, fps } });
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

  function persist(): Promise<void> {
    if (savingTask !== null) return savingTask;
    savingTask = ($appInfo.data.platform === "linux" ? saveCompatibility() : saveNative()).finally(() => { savingTask = null; });
    return savingTask;
  }

  async function saveCompatibility(): Promise<void> {
    const snapshot = compatibilitySnapshot;
    const draft = structuredClone($state.snapshot(overrides));
    const before = argumentsBefore;
    const after = argumentsAfter;
    const environment = environmentText;
    const dll = dllOverridesText;
    saving = true;
    saveError = null;
    saved = false;
    try {
      if (draft.prefixPath !== null && draft.prefixPath.trim().length === 0) {
        throw new Error(t("Seleziona una cartella per il prefix personalizzato.", $language));
      }
      const updated = await saveGameCompatibilityOverrides(game.id, {
        ...draft,
        argumentsBefore: draft.argumentsBefore === null ? null : parseLaunchArguments(before),
        argumentsAfter: draft.argumentsAfter === null ? null : parseLaunchArguments(after),
        environment: draft.environment === null ? null : parseEnvironment(environment, draft.debugLogging ?? defaults?.debugLogging ?? false),
        dllOverrides: draft.dllOverrides === null ? null : parseMap(dll, t("Override DLL", $language)),
      });
      baseline = JSON.stringify({ overrides: updated, argumentsBefore: before, argumentsAfter: after, environmentText: environment, dllOverridesText: dll });
      if (compatibilitySnapshot === snapshot) overrides = updated;
      failedSnapshot = null;
      saved = true;
    } catch (error) {
      failedSnapshot = snapshot;
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
  }

  function resetCompatibility(): void {
    saveError = null;
    saved = false;
    overrides = structuredClone(emptyGameCompatibilityOverrides);
    argumentsBefore = "";
    argumentsAfter = "";
    environmentText = "";
    dllOverridesText = "";
  }

  async function saveNative(): Promise<void> {
    const snapshot = nativeSnapshot;
    const argumentsText = nativeArguments;
    const workingDirectory = nativeWorkingDirectory;
    const environmentText = nativeEnvironmentText;
    saving = true;
    saveError = null;
    saved = false;
    try {
      const updated = await saveNativeLaunchConfig(game.id, {
        arguments: parseLaunchArguments(argumentsText),
        workingDirectory: workingDirectory || null,
        environment: parseMap(environmentText, t("Variabili ambiente", $language)),
      });
      baseline = JSON.stringify({ nativeArguments: formatLaunchArguments(updated.arguments), nativeWorkingDirectory: updated.workingDirectory ?? "", nativeEnvironmentText: mapToText(updated.environment) });
      if (nativeSnapshot === snapshot) {
        nativeConfig = updated;
        nativeArguments = formatLaunchArguments(updated.arguments);
        nativeWorkingDirectory = updated.workingDirectory ?? "";
        nativeEnvironmentText = mapToText(updated.environment);
      }
      failedSnapshot = null;
      saved = true;
    } catch (error) {
      failedSnapshot = snapshot;
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
  }
</script>

<Panel title={section === "locations" ? t("Percorsi del gioco", $language) : section === "launch" ? t("Opzioni di avvio", $language) : t("Compatibilità", $language)}>
  {#if loading}
    <p class="text-sm text-zinc-400" role="status">{t("Caricamento delle impostazioni di avvio...", $language)}</p>
  {:else if loadError !== null}
    <ErrorBanner message={loadError} onRetry={() => void load()} />
  {:else if $appInfo.data.platform === "linux" && section === "locations"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Il prefix predefinito è una cartella distinta per questo gioco sotto la radice globale. Avvio con Steam usa lo stesso percorso.", $language)}</p>
    <div class="grid gap-4">
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <Toggle label={t("Percorso del prefix personalizzato", $language)} checked={overrides.prefixPath !== null} onChange={togglePrefix} />
        <div class="flex gap-2">
          <div class="min-w-0 flex-1"><TextField id="game-compat-prefix" label={t("Cartella del prefix", $language)} value={overrides.prefixPath ?? ""} disabled={overrides.prefixPath === null} placeholder={t("Percorso assoluto", $language)} oninput={(value) => setOverride("prefixPath", value)} /></div>
          <Button label={t("Sfoglia...", $language)} variant="secondary" disabled={overrides.prefixPath === null} onClick={() => void browsePrefix()} />
        </div>
      </div>
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <Toggle label={t("Cartella di lavoro personalizzata", $language)} checked={overrides.workingDirectory !== null} onChange={toggleWorkingDirectory} />
        <TextField id="game-compat-working-directory" label={t("Cartella di lavoro", $language)} value={overrides.workingDirectory ?? ""} disabled={overrides.workingDirectory === null} placeholder={t("Predefinita: cartella dell'eseguibile", $language)} oninput={(value) => setOverride("workingDirectory", value)} />
      </div>
    </div>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Percorsi salvati.", $language)}</p>{/if}
  {:else if $appInfo.data.platform === "linux" && section === "compatibility"}
    <Toggle label={t("Avvio con Steam", $language)} checked={overrides.launchViaSteam ?? (game.steamAppId !== null)} onChange={(checked) => setOverride("launchViaSteam", checked)} />
    <p class="text-xs text-zinc-500">{t("Avvia Steam e il gioco con Proton usando il prefix della sezione Posizioni. Runtime e overlay vengono applicati automaticamente quando disponibili.", $language)}</p>
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n      Ogni campo eredita il default globale finché il relativo override resta disattivato. Una lista o una mappa vuota cancella il valore ereditato.\n    ", $language)}</p>
    {#if runnerDiagnostics.length > 0}
      <div class="rounded-lg bg-amber-500/10 p-3 text-sm text-amber-200 light:text-amber-900">
        {#each runnerDiagnostics as diagnostic (diagnostic)}<p>{diagnostic}</p>{/each}
      </div>
    {/if}
    <div class="flex flex-col gap-2">
      <h3 class="text-lg font-semibold text-zinc-100 light:text-zinc-900">{t("Versione Proton", $language)}</h3>
      {#if runnerChoices.length === 0}
        <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Nessun runner compatibile installato.", $language)}</p>
      {:else}
        <fieldset class="flex flex-col gap-1">
          <legend class="sr-only">{t("Runner di compatibilità", $language)}</legend>
          {#each runnerChoices as runner (runner.path)}
            <label class="flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-sm text-zinc-200 transition-colors hover:bg-white/5 has-checked:bg-white/10 light:text-zinc-800 light:hover:bg-zinc-900/5 light:has-checked:bg-zinc-900/10">
              <input type="radio" name="game-compat-runner" value={runner.path} checked={selectedRunnerPath === runner.path} onchange={() => selectRunner(runner.path)} class="size-4 accent-white" />
              <span class="min-w-0 truncate">{runner.name}{runner.kind === "wine" && runner.version ? ` ${runner.version}` : ""}</span>
              {#if runner.path === (defaults?.runnerPath ?? runners[0]?.path)}<span class="ml-auto shrink-0 text-xs text-zinc-500">{t("Predefinito", $language)}</span>{/if}
            </label>
          {/each}
        </fieldset>
      {/if}
    </div>
    <div class="grid gap-4 md:grid-cols-2">
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <span>{t("Variabili ambiente", $language)}</span>
        <Toggle label={t("Personalizza le variabili", $language)} checked={overrides.environment !== null} onChange={toggleEnvironment} />
        <textarea bind:value={environmentText} aria-label={t("Variabili ambiente", $language)} disabled={overrides.environment === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga. I controlli tipizzati gestiscono le variabili riservate.", $language)}</span>
      </div>
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <span>{t("Override DLL", $language)}</span>
        <Toggle label={t("Personalizza gli override", $language)} checked={overrides.dllOverrides !== null} onChange={toggleDllOverrides} />
        <textarea bind:value={dllOverridesText} aria-label={t("Override DLL", $language)} disabled={overrides.dllOverrides === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.", $language)}</span>
      </div>
    </div>

    <Toggle label={t("Avvia con OnlineFix", $language)} checked={overrides.onlineFix ?? onlineFixDetected}
      onChange={(checked) => setOverride("onlineFix", checked ? (onlineFixDetected ? null : true) : false)} />
    {#if onlineFixDetected}<p class="text-xs text-zinc-500">{t("OnlineFix64.dll rilevato nella cartella del gioco.", $language)}</p>{/if}

    <div class="flex flex-col gap-3">
      <span class="text-sm text-zinc-400 light:text-zinc-600">{t("Prestazioni Linux", $language)}</span>
      <Toggle label="GameMode" checked={overrides.linuxPerformance.gameMode}
        disabled={!gameModeAvailable && !overrides.linuxPerformance.gameMode}
        onChange={(checked) => setPerformance({ gameMode: checked })} />
      <Toggle label="Gamescope" checked={overrides.linuxPerformance.gamescope !== null}
        disabled={!gamescopeAvailable && overrides.linuxPerformance.gamescope === null}
        onChange={(checked) => setPerformance({ gamescope: checked ? { width: null, height: null, fps: null } : null })} />
      {#if !gameModeAvailable || !gamescopeAvailable}
        <p class="text-xs text-zinc-500">{t("Installa GameMode o gamescope e riapri queste impostazioni per abilitarli.", $language)}</p>
      {/if}
      {#if overrides.linuxPerformance.gamescope !== null}
        {@const config = overrides.linuxPerformance.gamescope}
        <div class="grid gap-4 md:grid-cols-2">
          <SelectField id="gamescope-resolution" label={t("Risoluzione interna", $language)}
            value={config.width === null ? "native" : `${config.width}x${config.height}`}
            options={[{ value: "native", label: t("Nativa", $language) }, ...[...new Set(["1280x720", "1600x900", "1920x1080", "2560x1440", "3840x2160", ...(config.width === null ? [] : [`${config.width}x${config.height}`])])].map((value) => ({ value, label: value }))]}
            onChange={setGamescopeResolution} />
          <SelectField id="gamescope-fps" label={t("Limite FPS", $language)} value={config.fps === null ? "uncapped" : String(config.fps)}
            options={[{ value: "uncapped", label: t("Senza limite", $language) }, ...[...new Set([30, 40, 60, 90, 120, 144, 165, 240, 360, ...(config.fps === null ? [] : [config.fps])])].map((fps) => ({ value: String(fps), label: String(fps) }))]}
            onChange={setGamescopeFps} />
        </div>
        <p class="text-xs text-zinc-500">{t("Una risoluzione inferiore riduce la qualità visiva. Il limite FPS può ridurre consumi e temperature.", $language)}</p>
      {/if}
      <p class="text-xs text-zinc-500">{t("GameMode e Gamescope sono facoltativi. Confronta prestazioni e frametime sul gioco; Wayland resta una scelta separata.", $language)}</p>
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <SelectField id="game-compat-renderer" label={t("Renderer grafico", $language)} value={overrides.graphicsRenderer ?? "inherit"} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "wine_d3d", label: "WineD3D" }]} onChange={setGraphicsRenderer} />
      <SelectField id="game-compat-wayland" label="Wayland" value={overrides.wayland ?? "inherit"} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "disabled", label: t("Disattivato", $language) }, { value: "native", label: t("Nativo, solo GE-Proton", $language) }]} onChange={setWayland} />
      <SelectField id="game-compat-debug" label={t("Log di debug", $language)} value={overrides.debugLogging === null ? "inherit" : String(overrides.debugLogging)} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "true", label: t("Attivi", $language) }, { value: "false", label: t("Disattivi", $language) }]} onChange={setDebugLogging} />
    </div>

    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Impostazioni salvate.", $language)}</p>{/if}
    <div class="flex flex-wrap gap-2">
      <Button label={t("Ripristina default globali", $language)} variant="secondary" disabled={saving} onClick={resetCompatibility} />
    </div>
  {:else if $appInfo.data.platform === "linux"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.", $language)}</p>
    <div class="grid gap-4 md:grid-cols-2">
      <div class="flex flex-col gap-2">
        <Toggle label={t("Prima dell'eseguibile", $language)} checked={overrides.argumentsBefore !== null} onChange={toggleArgumentsBefore} />
        <TextField id="arguments-before" label={t("Opzioni di avvio", $language)} value={argumentsBefore} disabled={overrides.argumentsBefore === null} placeholder="-windowed -novid" oninput={(value) => (argumentsBefore = value)} />
      </div>
      <div class="flex flex-col gap-2">
        <Toggle label={t("Dopo l'eseguibile", $language)} checked={overrides.argumentsAfter !== null} onChange={toggleArgumentsAfter} />
        <TextField id="arguments-after" label={t("Opzioni di avvio", $language)} value={argumentsAfter} disabled={overrides.argumentsAfter === null} placeholder="-windowed -novid" oninput={(value) => (argumentsAfter = value)} />
      </div>
    </div>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Argomenti salvati.", $language)}</p>{/if}
  {:else if $appInfo.data.platform === "windows" && section === "launch"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.", $language)}</p>
    <TextField id="native-arguments" label={t("Opzioni di avvio", $language)} value={nativeArguments} placeholder="-windowed -novid" oninput={(value) => (nativeArguments = value)} />
    <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">{t("Variabili ambiente", $language)}
      <textarea bind:value={nativeEnvironmentText} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
      <span class="text-xs text-zinc-500">{t("Una voce KEY=VALUE per riga. Applicate solo all'avvio di questo gioco.", $language)}</span>
    </label>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Argomenti salvati.", $language)}</p>{/if}
  {:else if $appInfo.data.platform === "windows"}
    {#if section === "locations"}
      <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Imposta la cartella iniziale del processo per questo gioco.", $language)}</p>
      <TextField id="native-working-directory" label={t("Cartella di lavoro", $language)} value={nativeWorkingDirectory} placeholder={t("Cartella dell'eseguibile", $language)} oninput={(value) => (nativeWorkingDirectory = value)} />
      {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
      {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Percorso salvato.", $language)}</p>{/if}
    {:else}
      <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Le opzioni di compatibilità aggiuntive non sono disponibili per i runner nativi. Configura gli argomenti di avvio in Generali.", $language)}</p>
    {/if}
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Le impostazioni di avvio manuale non sono disponibili su questa piattaforma.", $language)}</p>
  {/if}
</Panel>
