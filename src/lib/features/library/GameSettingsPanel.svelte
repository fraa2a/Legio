<script lang="ts">
  import { onMount } from "svelte";
  import type { Game } from "../../services/local-state";
  import {
    emptyGameCompatibilityOverrides,
    getCompatibilityDefaults,
    getGameCompatibilityOverrides,
    getNativeLaunchConfig,
    listCompatibilityRunners,
    saveGameCompatibilityOverrides,
    saveNativeLaunchConfig,
    type CompatibilityDefaults,
    type GameCompatibilityOverrides,
    type GraphicsRenderer,
    type NativeLaunchConfig,
    type WaylandMode,
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
      const [overrideResult, defaultsResult, runnersResult] = await Promise.allSettled([
        getGameCompatibilityOverrides(game.id),
        getCompatibilityDefaults(),
        listCompatibilityRunners(),
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

  function mapToText(values: Record<string, string>): string {
    return Object.entries(values)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, value]) => `${key}=${value}`)
      .join("\n");
  }

  function parseMap(value: string, label: string): Record<string, string> {
    const result: Record<string, string> = {};
    for (const line of value.split(/\r?\n/)) {
      if (line.trim().length === 0) continue;
      const separator = line.indexOf("=");
      const key = separator < 0 ? "" : line.slice(0, separator).trim();
      if (key.length === 0) throw new Error(`${label}: ogni riga deve contenere una chiave e un valore separati da =.`);
      if (Object.hasOwn(result, key)) throw new Error(`${label}: la chiave ${key} è ripetuta.`);
      result[key] = line.slice(separator + 1);
    }
    return result;
  }

  function parseEnvironment(value: string): Record<string, string> {
    const environment = parseMap(value, "Ambiente");
    const typedKeys = [
      "WINEPREFIX",
      "STEAM_COMPAT_DATA_PATH",
      "STEAM_COMPAT_CLIENT_INSTALL_PATH",
      "WINEDLLOVERRIDES",
      "PROTON_USE_WINED3D",
      "PROTON_ENABLE_WAYLAND",
      "ENABLE_VK_LAYER_VALVE_steam_overlay_1",
      "SteamOverlayGameId",
    ];
    const debugLogging = overrides.debugLogging ?? defaults?.debugLogging ?? false;
    for (const key of Object.keys(environment)) {
      if (typedKeys.some((managed) => key.toLowerCase() === managed.toLowerCase())) {
        throw new Error(`La variabile ${key} è gestita dalle opzioni tipizzate di Legio.`);
      }
      if (debugLogging && ["PROTON_LOG", "PROTON_LOG_DIR", "SteamGameId"].some((managed) => key.toLowerCase() === managed.toLowerCase())) {
        throw new Error(`La variabile ${key} è gestita dai log di debug.`);
      }
    }
    return environment;
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
      const path = await pickGameDirectory(overrides.prefixPath, "Seleziona la cartella del prefix");
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
    setOverride("graphicsRenderer", value === "inherit" ? null : (value as GraphicsRenderer));
  }

  function setWayland(value: string): void {
    setOverride("wayland", value === "inherit" ? null : (value as WaylandMode));
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
        throw new Error("Seleziona una cartella per il prefix personalizzato.");
      }
      overrides = await saveGameCompatibilityOverrides(game.id, {
        ...overrides,
        argumentsBefore: overrides.argumentsBefore === null ? null : parseLaunchArguments(argumentsBefore),
        argumentsAfter: overrides.argumentsAfter === null ? null : parseLaunchArguments(argumentsAfter),
        environment: overrides.environment === null ? null : parseEnvironment(environmentText),
        dllOverrides: overrides.dllOverrides === null ? null : parseMap(dllOverridesText, "Override DLL"),
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

<Panel title={section === "locations" ? "Percorsi del gioco" : section === "launch" ? "Launch Options" : "Compatibilità"}>
  {#if loading}
    <p class="text-sm text-zinc-400" role="status">Caricamento delle impostazioni di avvio...</p>
  {:else if loadError !== null}
    <ErrorBanner message={loadError} onRetry={() => void load()} />
  {:else if $appInfo.data.platform === "linux" && section === "locations"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">Il prefix predefinito è una cartella distinta per questo gioco sotto la radice globale. Avvio con Steam usa lo stesso percorso.</p>
    <div class="grid gap-4">
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <span class="flex items-center gap-2">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.prefixPath !== null} onchange={(event) => togglePrefix(event.currentTarget.checked)} />
          Percorso del prefix personalizzato
        </span>
        <div class="flex gap-2">
          <div class="min-w-0 flex-1"><TextField id="game-compat-prefix" label="Cartella del prefix" value={overrides.prefixPath ?? ""} disabled={overrides.prefixPath === null} placeholder="Percorso assoluto" oninput={(value) => setOverride("prefixPath", value)} /></div>
          <Button label="Sfoglia..." variant="secondary" disabled={overrides.prefixPath === null} onClick={() => void browsePrefix()} />
        </div>
      </div>
      <div class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <span class="flex items-center gap-2">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.workingDirectory !== null} onchange={(event) => toggleWorkingDirectory(event.currentTarget.checked)} />
          Cartella di lavoro personalizzata
        </span>
        <TextField id="game-compat-working-directory" label="Cartella di lavoro" value={overrides.workingDirectory ?? ""} disabled={overrides.workingDirectory === null} placeholder="Predefinita: cartella dell'eseguibile" oninput={(value) => setOverride("workingDirectory", value)} />
      </div>
    </div>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Percorsi salvati.</p>{/if}
    <div><Button label={saving ? "Salvataggio..." : "Salva percorsi"} disabled={saving} onClick={() => void saveCompatibility()} /></div>
  {:else if $appInfo.data.platform === "linux" && section === "compatibility"}
    {#if game.steamAppId !== null}
      <label class="flex items-center gap-2 text-sm text-zinc-200 light:text-zinc-800">
        <input type="checkbox" class="size-4 accent-white" checked={overrides.launchViaSteam ?? true}
          onchange={(event) => setOverride("launchViaSteam", event.currentTarget.checked)} />
        Avvio con Steam
      </label>
      <p class="text-xs text-zinc-500">Avvia Steam e il gioco con Proton usando il prefix della sezione Posizioni. Runtime e overlay vengono applicati automaticamente quando disponibili.</p>
    {/if}
    <p class="text-sm text-zinc-400 light:text-zinc-600">
      Ogni campo eredita il default globale finché il relativo override resta disattivato. Una lista o una mappa vuota cancella il valore ereditato.
    </p>
    {#if runnerDiagnostics.length > 0}
      <div class="rounded-lg bg-amber-500/10 p-3 text-sm text-amber-200 light:text-amber-900">
        {#each runnerDiagnostics as diagnostic (diagnostic)}<p>{diagnostic}</p>{/each}
      </div>
    {/if}
    <div class="flex flex-col gap-2">
      <h3 class="text-lg font-semibold text-zinc-100 light:text-zinc-900">Proton Version</h3>
      {#if runnerChoices.length === 0}
        <p class="text-sm text-zinc-400 light:text-zinc-600">Nessun runner compatibile installato.</p>
      {:else}
        <fieldset class="flex flex-col gap-1">
          <legend class="sr-only">Runner di compatibilità</legend>
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
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Variabili ambiente
        <span class="flex items-center gap-2 text-xs">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.environment !== null} onchange={(event) => toggleEnvironment(event.currentTarget.checked)} />
          Personalizza le variabili
        </span>
        <textarea bind:value={environmentText} disabled={overrides.environment === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Una voce KEY=VALUE per riga. I controlli tipizzati gestiscono le variabili riservate.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Override DLL
        <span class="flex items-center gap-2 text-xs">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.dllOverrides !== null} onchange={(event) => toggleDllOverrides(event.currentTarget.checked)} />
          Personalizza gli override
        </span>
        <textarea bind:value={dllOverridesText} disabled={overrides.dllOverrides === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.</span>
      </label>
    </div>

    <label class="flex items-center gap-2 text-sm text-zinc-200 light:text-zinc-800">
      <input type="checkbox" class="size-4 accent-white" checked={overrides.onlineFix ?? true}
        onchange={(event) => setOverride("onlineFix", event.currentTarget.checked)} />
      Avvia con OnlineFix
    </label>

    <div class="grid gap-4 md:grid-cols-2">
      <SelectField id="game-compat-renderer" label="Renderer grafico" value={overrides.graphicsRenderer ?? "inherit"} options={[{ value: "inherit", label: "Eredita default" }, { value: "runner_default", label: "Predefinito del runner" }, { value: "wine_d3d", label: "WineD3D" }]} onChange={setGraphicsRenderer} />
      <SelectField id="game-compat-wayland" label="Wayland" value={overrides.wayland ?? "inherit"} options={[{ value: "inherit", label: "Eredita default" }, { value: "runner_default", label: "Predefinito del runner" }, { value: "disabled", label: "Disattivato" }, { value: "native", label: "Nativo, solo GE-Proton" }]} onChange={setWayland} />
      <SelectField id="game-compat-debug" label="Log di debug" value={overrides.debugLogging === null ? "inherit" : String(overrides.debugLogging)} options={[{ value: "inherit", label: "Eredita default" }, { value: "true", label: "Attivi" }, { value: "false", label: "Disattivi" }]} onChange={setDebugLogging} />
    </div>

    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Impostazioni salvate.</p>{/if}
    <div class="flex flex-wrap gap-2">
      <Button label={saving ? "Salvataggio..." : "Salva impostazioni"} disabled={saving} onClick={() => void saveCompatibility()} />
      <Button label="Ripristina default globali" variant="secondary" disabled={saving} onClick={() => void resetCompatibility()} />
    </div>
  {:else if $appInfo.data.platform === "linux"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.</p>
    <div class="grid gap-4 md:grid-cols-2">
      <div class="flex flex-col gap-2">
        <label class="flex items-center gap-2 text-sm text-zinc-300 light:text-zinc-700">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.argumentsBefore !== null} onchange={(event) => toggleArgumentsBefore(event.currentTarget.checked)} />
          Prima dell'eseguibile
        </label>
        <TextField id="arguments-before" label="Launch Options" value={argumentsBefore} disabled={overrides.argumentsBefore === null} placeholder="-windowed -novid" oninput={(value) => (argumentsBefore = value)} />
      </div>
      <div class="flex flex-col gap-2">
        <label class="flex items-center gap-2 text-sm text-zinc-300 light:text-zinc-700">
          <input type="checkbox" class="size-4 accent-white" checked={overrides.argumentsAfter !== null} onchange={(event) => toggleArgumentsAfter(event.currentTarget.checked)} />
          Dopo l'eseguibile
        </label>
        <TextField id="arguments-after" label="Launch Options" value={argumentsAfter} disabled={overrides.argumentsAfter === null} placeholder="-windowed -novid" oninput={(value) => (argumentsAfter = value)} />
      </div>
    </div>
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Argomenti salvati.</p>{/if}
    <div><Button label={saving ? "Salvataggio..." : "Salva opzioni"} disabled={saving} onClick={() => void saveCompatibility()} /></div>
  {:else if $appInfo.data.platform === "windows" && section === "launch"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">Aggiungi argomenti di avvio. Racchiudi tra virgolette i valori che contengono spazi.</p>
    <TextField id="native-arguments" label="Launch Options" value={nativeArguments} placeholder="-windowed -novid" oninput={(value) => (nativeArguments = value)} />
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Argomenti salvati.</p>{/if}
    <div><Button label={saving ? "Salvataggio..." : "Salva opzioni"} disabled={saving} onClick={() => void saveNative()} /></div>
  {:else if $appInfo.data.platform === "windows"}
    {#if section === "locations"}
      <p class="text-sm text-zinc-400 light:text-zinc-600">Imposta la cartella iniziale del processo per questo gioco.</p>
      <TextField id="native-working-directory" label="Cartella di lavoro" value={nativeWorkingDirectory} placeholder="Cartella dell'eseguibile" oninput={(value) => (nativeWorkingDirectory = value)} />
      {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
      {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Percorso salvato.</p>{/if}
      <div><Button label={saving ? "Salvataggio..." : "Salva percorso"} disabled={saving} onClick={() => void saveNative()} /></div>
    {:else}
      <p class="text-sm text-zinc-400 light:text-zinc-600">Le opzioni di compatibilità aggiuntive non sono disponibili per i runner nativi. Configura gli argomenti di avvio in Generali.</p>
    {/if}
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">Le impostazioni di avvio manuale non sono disponibili su questa piattaforma.</p>
  {/if}
</Panel>
