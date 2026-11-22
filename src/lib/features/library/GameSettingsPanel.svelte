<script lang="ts">
  import { mapToText, parseMap } from "../../services/launch-fields";
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
    type LaunchSettingsCache,
    type NativeLaunchConfig,
  } from "../../services/game-settings";
  import { appInfo } from "../../stores/app-info";
  import { usesCompatibilityRunner } from "./library-model";
  import Button from "../../components/ui/Button.svelte";
  import RadioCard from "../../components/ui/RadioCard.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import LinuxPerformanceFields from "./LinuxPerformanceFields.svelte";
  import { createSaveQueue } from "../../services/save-queue";
  import { toMessage } from "../../utils/errors";
  import { pickGameDirectory } from "../../services/dialog";
  import { formatLaunchArguments, parseLaunchArguments } from "../../utils/launch-arguments";

  let { game, section, cache }: { game: Game; section: "locations" | "launch" | "compatibility"; cache: LaunchSettingsCache } = $props();

  async function cached<K extends keyof LaunchSettingsCache>(key: K, fetch: () => Promise<NonNullable<LaunchSettingsCache[K]>>): Promise<NonNullable<LaunchSettingsCache[K]>> {
    return (cache[key] ??= await fetch()) as NonNullable<LaunchSettingsCache[K]>;
  }

  let overrides = $state<GameCompatibilityOverrides>({ ...emptyGameCompatibilityOverrides });
  let onlineFixDetected = $state(false);
  let defaults = $state<CompatibilityDefaults | null>(null);
  let runners = $state<{ kind: string; name: string; version: string; path: string }[]>([]);
  let runnerDiagnostics = $state<string[]>([]);
  let gameModeAvailable = $state(false);
  let gamescopeAvailable = $state(false);
  let mangohudAvailable = $state(false);
  let wrapperText = $state("");
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
  let baseline = $state<string | null>(null);
  let failedSnapshot: string | null = null;
  const saveQueue = createSaveQueue();
  let disposed = false;

  function compatibilityDraft() {
    return { overrides: structuredClone($state.snapshot(overrides)), argumentsBefore, argumentsAfter, environmentText, dllOverridesText, wrapperText };
  }

  function nativeDraft() {
    return { nativeArguments, nativeWorkingDirectory, nativeEnvironmentText };
  }
  const compatibilitySnapshot = $derived(JSON.stringify({ overrides, argumentsBefore, argumentsAfter, environmentText, dllOverridesText, wrapperText }));
  const nativeSnapshot = $derived(JSON.stringify({ nativeArguments, nativeWorkingDirectory, nativeEnvironmentText }));
  const compatibilityGame = $derived(usesCompatibilityRunner(game, $appInfo.data.platform));
  const currentSnapshot = $derived(compatibilityGame ? compatibilitySnapshot : nativeSnapshot);

  onMount(() => {
    void load();
  });

  $effect(() => {
    if (loading || loadError !== null || baseline === null || saving || currentSnapshot === baseline || currentSnapshot === failedSnapshot) return;
    const timer = setTimeout(() => void persist(), 550);
    return () => clearTimeout(timer);
  });

  onDestroy(() => {
    const compatibility = compatibilityGame;
    const snapshot = JSON.stringify(compatibility ? compatibilityDraft() : nativeDraft());
    const shouldSave = !loading && loadError === null && baseline !== null;
    disposed = true;
    void saveQueue.flush(async () => {
      if (shouldSave && snapshot !== baseline && snapshot !== failedSnapshot) await persist(snapshot, compatibility);
    });
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
    if (compatibilityGame) {
      const [overrideResult, defaultsResult, runnersResult, onlineFixResult] = await Promise.allSettled([
        cached("overrides", () => getGameCompatibilityOverrides(game.id)),
        cached("defaults", getCompatibilityDefaults),
        section === "compatibility" ? cached("runners", listCompatibilityRunners) : Promise.resolve({ runners: [], diagnostics: [], gameModeAvailable: false, mangohudAvailable: false, gamescopeAvailable: false }),
        section === "compatibility" ? cached("onlineFix", () => getGameOnlineFixDetected(game.id)) : Promise.resolve(false),
      ]);
      if (overrideResult.status === "fulfilled") {
        overrides = overrideResult.value;
        argumentsBefore = formatLaunchArguments(overrides.argumentsBefore ?? []);
        argumentsAfter = formatLaunchArguments(overrides.argumentsAfter ?? []);
        environmentText = mapToText(overrides.environment ?? {});
        dllOverridesText = mapToText(overrides.dllOverrides ?? {});
        wrapperText = formatLaunchArguments(overrides.linuxPerformance.wrapper);
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
        mangohudAvailable = runnersResult.value.mangohudAvailable;
      } else if (loadError === null) {
        loadError = toMessage(runnersResult.reason);
      }
      if (onlineFixResult.status === "fulfilled") onlineFixDetected = onlineFixResult.value;
      else if (loadError === null) loadError = toMessage(onlineFixResult.reason);
    } else if (platform === "windows" || platform === "linux") {
      try {
        nativeConfig = await cached("native", () => getNativeLaunchConfig(game.id));
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

  function persist(snapshot?: string, compatibility = compatibilityGame): Promise<void> {
    return saveQueue.run(() => compatibility ? saveCompatibility(snapshot) : saveNative(snapshot));
  }

  async function saveCompatibility(captured?: string): Promise<void> {
    const values: ReturnType<typeof compatibilityDraft> = captured === undefined ? compatibilityDraft() : JSON.parse(captured);
    const snapshot = JSON.stringify(values);
    const { overrides: draft, argumentsBefore: before, argumentsAfter: after, environmentText: environment, dllOverridesText: dll, wrapperText: wrapper } = values;
    saving = true;
    saveError = null;
    try {
      if (draft.prefixPath !== null && draft.prefixPath.trim().length === 0) {
        throw new Error(t("Seleziona una cartella per il prefix personalizzato.", $language));
      }
      const updated = await saveGameCompatibilityOverrides(game.id, {
        ...draft,
        argumentsBefore: draft.argumentsBefore === null ? null : parseLaunchArguments(before),
        argumentsAfter: draft.argumentsAfter === null ? null : parseLaunchArguments(after),
        linuxPerformance: { ...draft.linuxPerformance, wrapper: parseLaunchArguments(wrapper) },
        environment: draft.environment === null ? null : parseMap(environment, t("Variabili ambiente", $language)),
        dllOverrides: draft.dllOverrides === null ? null : parseMap(dll, t("Override DLL", $language)),
      });
      baseline = JSON.stringify({ overrides: updated, argumentsBefore: before, argumentsAfter: after, environmentText: environment, dllOverridesText: dll, wrapperText: wrapper });
      cache.overrides = updated;
      if (!disposed && compatibilitySnapshot === snapshot) overrides = updated;
      failedSnapshot = null;
    } catch (error) {
      failedSnapshot = snapshot;
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
  }

  function resetCompatibility(): void {
    saveError = null;
    overrides = structuredClone(emptyGameCompatibilityOverrides);
    argumentsBefore = "";
    argumentsAfter = "";
    environmentText = "";
    dllOverridesText = "";
    wrapperText = "";
  }

  async function saveNative(captured?: string): Promise<void> {
    const values: ReturnType<typeof nativeDraft> = captured === undefined ? nativeDraft() : JSON.parse(captured);
    const snapshot = JSON.stringify(values);
    const { nativeArguments: argumentsText, nativeWorkingDirectory: workingDirectory, nativeEnvironmentText: environmentText } = values;
    saving = true;
    saveError = null;
    try {
      const updated = await saveNativeLaunchConfig(game.id, {
        arguments: parseLaunchArguments(argumentsText),
        workingDirectory: workingDirectory || null,
        environment: parseMap(environmentText, t("Variabili ambiente", $language)),
      });
      baseline = JSON.stringify({ nativeArguments: formatLaunchArguments(updated.arguments), nativeWorkingDirectory: updated.workingDirectory ?? "", nativeEnvironmentText: mapToText(updated.environment) });
      cache.native = updated;
      if (!disposed && nativeSnapshot === snapshot) {
        nativeConfig = updated;
        nativeArguments = formatLaunchArguments(updated.arguments);
        nativeWorkingDirectory = updated.workingDirectory ?? "";
        nativeEnvironmentText = mapToText(updated.environment);
      }
      failedSnapshot = null;
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
    <p class="text-sm text-legio-fg-muted" role="status">{t("Caricamento delle impostazioni di avvio...", $language)}</p>
  {:else if loadError !== null}
    <ErrorBanner message={loadError} onRetry={() => void load()} />
  {:else if compatibilityGame && section === "locations"}
    <p class="text-sm text-legio-fg-muted">{t("Il prefix predefinito è una cartella distinta per questo gioco sotto la radice globale. Avvio con Steam usa lo stesso percorso.", $language)}</p>
    <div class="grid gap-4">
      <div class="flex flex-col gap-1.5 text-sm text-legio-fg-muted">
        <Toggle label={t("Percorso del prefix personalizzato", $language)} checked={overrides.prefixPath !== null} onChange={togglePrefix} />
        <div class="flex gap-2">
          <div class="min-w-0 flex-1"><TextField id="game-compat-prefix" label={t("Cartella del prefix", $language)} value={overrides.prefixPath ?? ""} disabled={overrides.prefixPath === null} placeholder={t("Percorso assoluto", $language)} oninput={(value) => setOverride("prefixPath", value)} /></div>
          <Button label={t("Sfoglia...", $language)} variant="secondary" disabled={overrides.prefixPath === null} onClick={() => void browsePrefix()} />
        </div>
      </div>
      <div class="flex flex-col gap-1.5 text-sm text-legio-fg-muted">
        <Toggle label={t("Cartella di lavoro personalizzata", $language)} checked={overrides.workingDirectory !== null} onChange={toggleWorkingDirectory} />
        <TextField id="game-compat-working-directory" label={t("Cartella di lavoro", $language)} value={overrides.workingDirectory ?? ""} disabled={overrides.workingDirectory === null} placeholder={t("Predefinita: cartella dell'eseguibile", $language)} oninput={(value) => setOverride("workingDirectory", value)} />
      </div>
    </div>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
  {:else if compatibilityGame && section === "compatibility"}
    <Toggle label={t("Avvio con Steam", $language)} checked={overrides.launchViaSteam ?? (game.steamAppId !== null)} onChange={(checked) => setOverride("launchViaSteam", checked)} />
    <p class="text-xs text-legio-fg-subtle">{t("Avvia Steam e il gioco con Proton usando il prefix della sezione Posizioni. Runtime e overlay vengono applicati automaticamente quando disponibili.", $language)}</p>
    <p class="text-sm text-legio-fg-muted">{t("Ogni campo eredita il default globale finché il relativo override resta disattivato. Una lista o una mappa vuota cancella il valore ereditato.", $language)}</p>
    {#if runnerDiagnostics.length > 0}
      <Callout tone="warning" title={t("Rilevamento runner", $language)} items={runnerDiagnostics} />
    {/if}
    <div class="flex flex-col gap-2">
      <h3 class="text-sm font-semibold text-legio-fg">{t("Versione Proton", $language)}</h3>
      {#if runnerChoices.length === 0}
        <p class="text-sm text-legio-fg-muted">{t("Nessun runner compatibile installato.", $language)}</p>
      {:else}
        <fieldset class="flex flex-col gap-1">
          <legend class="sr-only">{t("Runner di compatibilità", $language)}</legend>
          {#each runnerChoices as runner (runner.path)}
            <RadioCard name="game-compat-runner" value={runner.path} checked={selectedRunnerPath === runner.path} onSelect={() => selectRunner(runner.path)}>
              <span class="min-w-0 truncate">{runner.name}{runner.kind === "wine" && runner.version ? ` ${runner.version}` : ""}</span>
              {#if runner.path === (defaults?.runnerPath ?? runners[0]?.path)}<span class="ml-auto shrink-0 text-xs text-legio-fg-subtle">{t("Predefinito", $language)}</span>{/if}
            </RadioCard>
          {/each}
        </fieldset>
      {/if}
    </div>
    <div class="grid gap-4 md:grid-cols-2">
      <div class="flex flex-col gap-1.5 text-sm text-legio-fg-muted">
        <Toggle label={t("Personalizza le variabili", $language)} checked={overrides.environment !== null} onChange={toggleEnvironment} />
        <TextField id="game-environmentText" type="textarea" rows={5} label={t("Variabili ambiente", $language)} bind:value={environmentText} disabled={overrides.environment === null} />
        <span class="text-xs text-legio-fg-subtle">{t("Una voce KEY=VALUE per riga. I controlli tipizzati gestiscono le variabili riservate.", $language)}</span>
      </div>
      <div class="flex flex-col gap-1.5 text-sm text-legio-fg-muted">
        <Toggle label={t("Personalizza gli override", $language)} checked={overrides.dllOverrides !== null} onChange={toggleDllOverrides} />
        <TextField id="game-dllOverridesText" type="textarea" rows={5} label={t("Override DLL", $language)} bind:value={dllOverridesText} disabled={overrides.dllOverrides === null} />
        <span class="text-xs text-legio-fg-subtle">{t("Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.", $language)}</span>
      </div>
    </div>

    <Toggle label={t("Avvia con OnlineFix", $language)} checked={overrides.onlineFix ?? onlineFixDetected}
      onChange={(checked) => setOverride("onlineFix", checked ? (onlineFixDetected ? null : true) : false)} />
    {#if onlineFixDetected}<p class="text-xs text-legio-fg-subtle">{t("OnlineFix64.dll rilevato nella cartella del gioco.", $language)}</p>{/if}

    <LinuxPerformanceFields value={overrides.linuxPerformance} {gameModeAvailable} {mangohudAvailable} {gamescopeAvailable} idPrefix="game"
      onChange={(value) => setOverride("linuxPerformance", value)} />
    <div class="flex flex-col gap-3">
      <TextField id="linux-wrapper-command" label={t("Comando wrapper personalizzato", $language)} value={wrapperText} placeholder="prime-run" oninput={(value) => (wrapperText = value)} />
      <p class="text-xs text-legio-fg-subtle">{t("Prefisso eseguito prima del gioco, ad esempio prime-run o taskset -c 0-7. Non passa da una shell.", $language)}</p>
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <SelectField id="game-compat-renderer" label={t("Renderer grafico", $language)} value={overrides.graphicsRenderer ?? "inherit"} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "wine_d3d", label: "WineD3D" }]} onChange={setGraphicsRenderer} />
      <SelectField id="game-compat-wayland" label="Wayland" value={overrides.wayland ?? "inherit"} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "runner_default", label: t("Predefinito del runner", $language) }, { value: "disabled", label: t("Disattivato", $language) }, { value: "native", label: t("Nativo, solo GE-Proton", $language) }]} onChange={setWayland} />
      <SelectField id="game-compat-debug" label={t("Log di debug", $language)} value={overrides.debugLogging === null ? "inherit" : String(overrides.debugLogging)} options={[{ value: "inherit", label: t("Eredita default", $language) }, { value: "true", label: t("Attivi", $language) }, { value: "false", label: t("Disattivi", $language) }]} onChange={setDebugLogging} />
    </div>

    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    <div class="flex flex-wrap gap-2">
      <Button label={t("Ripristina default globali", $language)} variant="secondary" disabled={saving} onClick={resetCompatibility} />
    </div>
  {:else if compatibilityGame}
    <p class="text-sm text-legio-fg-muted">{t("Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.", $language)}</p>
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
  {:else if !compatibilityGame && section === "launch"}
    <p class="text-sm text-legio-fg-muted">{t("Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.", $language)}</p>
    <TextField id="native-arguments" label={t("Opzioni di avvio", $language)} value={nativeArguments} placeholder="-windowed -novid" oninput={(value) => (nativeArguments = value)} />
    <TextField id="native-environment" type="textarea" rows={4} label={t("Variabili ambiente", $language)} bind:value={nativeEnvironmentText} hint={t("Una voce KEY=VALUE per riga. Applicate solo all'avvio di questo gioco.", $language)} />
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
  {:else if !compatibilityGame}
    {#if section === "locations"}
      <p class="text-sm text-legio-fg-muted">{t("Imposta la cartella iniziale del processo per questo gioco.", $language)}</p>
      <TextField id="native-working-directory" label={t("Cartella di lavoro", $language)} value={nativeWorkingDirectory} placeholder={t("Cartella dell'eseguibile", $language)} oninput={(value) => (nativeWorkingDirectory = value)} />
      {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {:else}
      <p class="text-sm text-legio-fg-muted">{t("Le opzioni di compatibilità aggiuntive non sono disponibili per i runner nativi. Configura gli argomenti di avvio in Generali.", $language)}</p>
    {/if}
  {:else}
    <p class="text-sm text-legio-fg-muted">{t("Le impostazioni di avvio manuale non sono disponibili su questa piattaforma.", $language)}</p>
  {/if}
</Panel>
