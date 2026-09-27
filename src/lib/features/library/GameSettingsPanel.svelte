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
    type SteamOverlayMode,
    type SteamRuntimeMode,
    type WaylandMode,
  } from "../../services/game-settings";
  import { appInfo } from "../../stores/app-info";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { toMessage } from "../../utils/errors";

  let { game }: { game: Game } = $props();

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
        argumentsBefore = overrides.argumentsBefore?.join("\n") ?? "";
        argumentsAfter = overrides.argumentsAfter?.join("\n") ?? "";
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
        nativeArguments = nativeConfig.arguments.join("\n");
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

  function parseArguments(value: string): string[] {
    return value === "" ? [] : value.split("\n").map((line) => line.replace(/\r$/, ""));
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

  function toggleRunner(enabled: boolean): void {
    setOverride("runnerPath", enabled ? defaults?.runnerPath ?? "" : null);
  }

  function togglePrefix(enabled: boolean): void {
    setOverride("prefixPath", enabled ? "" : null);
  }

  function toggleWorkingDirectory(enabled: boolean): void {
    setOverride("workingDirectory", enabled ? defaults?.workingDirectory ?? "" : null);
  }

  function toggleArgumentsBefore(enabled: boolean): void {
    const value = enabled ? [...(defaults?.argumentsBefore ?? [])] : null;
    setOverride("argumentsBefore", value);
    argumentsBefore = value?.join("\n") ?? "";
  }

  function toggleArgumentsAfter(enabled: boolean): void {
    const value = enabled ? [...(defaults?.argumentsAfter ?? [])] : null;
    setOverride("argumentsAfter", value);
    argumentsAfter = value?.join("\n") ?? "";
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

  function setSteamRuntime(value: string): void {
    setOverride("steamRuntime", value === "inherit" ? null : (value as SteamRuntimeMode));
  }

  function setSteamOverlay(value: string): void {
    setOverride("steamOverlay", value === "inherit" ? null : (value as SteamOverlayMode));
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

  const runnerOptions = $derived.by(() => {
    const values = [{ value: "", label: "Nessun runner" }];
    if (overrides.runnerPath && !runners.some((runner) => runner.path === overrides.runnerPath)) {
      values.push({ value: overrides.runnerPath, label: overrides.runnerPath });
    }
    values.push(...runners.map((runner) => ({ value: runner.path, label: `${runner.name} (${runner.version})` })));
    return values;
  });

  async function saveCompatibility(): Promise<void> {
    saving = true;
    saveError = null;
    saved = false;
    try {
      overrides = await saveGameCompatibilityOverrides(game.id, {
        ...overrides,
        argumentsBefore: overrides.argumentsBefore === null ? null : parseArguments(argumentsBefore),
        argumentsAfter: overrides.argumentsAfter === null ? null : parseArguments(argumentsAfter),
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
        arguments: parseArguments(nativeArguments),
        workingDirectory: nativeWorkingDirectory || null,
      });
      nativeArguments = nativeConfig.arguments.join("\n");
      nativeWorkingDirectory = nativeConfig.workingDirectory ?? "";
      saved = true;
    } catch (error) {
      saveError = toMessage(error);
    } finally {
      saving = false;
    }
  }
</script>

<Panel title="Avvio e compatibilità">
  {#if loading}
    <p class="text-sm text-zinc-400" role="status">Caricamento delle impostazioni di avvio...</p>
  {:else if loadError !== null}
    <ErrorBanner message={loadError} onRetry={() => void load()} />
  {:else if $appInfo.data.platform === "linux"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">
      Ogni campo eredita il default globale finché il relativo override resta disattivato. Una lista o una mappa vuota cancella il valore ereditato.
    </p>
    {#if runnerDiagnostics.length > 0}
      <div class="rounded-lg bg-amber-500/10 p-3 text-sm text-amber-200 light:text-amber-900">
        {#each runnerDiagnostics as diagnostic (diagnostic)}<p>{diagnostic}</p>{/each}
      </div>
    {/if}

    <div class="grid gap-4 md:grid-cols-2">
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        <span class="flex items-center gap-2">
          <input type="checkbox" checked={overrides.runnerPath !== null} onchange={(event) => toggleRunner(event.currentTarget.checked)} />
          Runner personalizzato
        </span>
        <select
          value={overrides.runnerPath ?? ""}
          disabled={overrides.runnerPath === null}
          onchange={(event) => setOverride("runnerPath", event.currentTarget.value)}
          class="h-10 rounded-lg bg-white/5 px-3 text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"
        >
          {#each runnerOptions as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
        </select>
      </label>
      <div class="flex flex-col gap-1.5">
        <label class="flex items-center gap-2 text-sm text-zinc-400 light:text-zinc-600">
          <input type="checkbox" checked={overrides.prefixPath !== null} onchange={(event) => togglePrefix(event.currentTarget.checked)} />
          Prefix dedicato
        </label>
        <TextField
          id="game-compat-prefix"
          label="Percorso del prefix"
          value={overrides.prefixPath ?? ""}
          disabled={overrides.prefixPath === null}
          placeholder="Percorso opzionale"
          oninput={(value) => setOverride("prefixPath", value)}
        />
      </div>
      <div class="flex flex-col gap-1.5">
        <label class="flex items-center gap-2 text-sm text-zinc-400 light:text-zinc-600">
          <input type="checkbox" checked={overrides.workingDirectory !== null} onchange={(event) => toggleWorkingDirectory(event.currentTarget.checked)} />
          Cartella di lavoro personalizzata
        </label>
        <TextField
          id="game-compat-working-directory"
          label="Cartella di lavoro"
          value={overrides.workingDirectory ?? ""}
          disabled={overrides.workingDirectory === null}
          placeholder="Eredita il default"
          oninput={(value) => setOverride("workingDirectory", value)}
        />
      </div>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Argomenti prima dell'eseguibile
        <span class="flex items-center gap-2 text-xs">
          <input type="checkbox" checked={overrides.argumentsBefore !== null} onchange={(event) => toggleArgumentsBefore(event.currentTarget.checked)} />
          Personalizza gli argomenti
        </span>
        <textarea bind:value={argumentsBefore} disabled={overrides.argumentsBefore === null} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Un argomento per riga, senza interpretazione shell.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Argomenti dopo l'eseguibile
        <span class="flex items-center gap-2 text-xs">
          <input type="checkbox" checked={overrides.argumentsAfter !== null} onchange={(event) => toggleArgumentsAfter(event.currentTarget.checked)} />
          Personalizza gli argomenti
        </span>
        <textarea bind:value={argumentsAfter} disabled={overrides.argumentsAfter === null} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Un argomento per riga, senza interpretazione shell.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Variabili ambiente
        <span class="flex items-center gap-2 text-xs">
          <input type="checkbox" checked={overrides.environment !== null} onchange={(event) => toggleEnvironment(event.currentTarget.checked)} />
          Personalizza le variabili
        </span>
        <textarea bind:value={environmentText} disabled={overrides.environment === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Una voce KEY=VALUE per riga. I controlli tipizzati gestiscono le variabili riservate.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Override DLL
        <span class="flex items-center gap-2 text-xs">
          <input type="checkbox" checked={overrides.dllOverrides !== null} onchange={(event) => toggleDllOverrides(event.currentTarget.checked)} />
          Personalizza gli override
        </span>
        <textarea bind:value={dllOverridesText} disabled={overrides.dllOverrides === null} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.</span>
      </label>
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Steam Linux Runtime
        <select value={overrides.steamRuntime ?? "inherit"} onchange={(event) => setSteamRuntime(event.currentTarget.value)} class="h-10 rounded-lg bg-white/5 px-3 text-sm text-zinc-100 light:bg-white light:text-zinc-900">
          <option value="inherit">Eredita default</option>
          <option value="runner_default">Predefinito del runner</option>
          <option value="steam_linux_runtime">Steam Linux Runtime</option>
        </select>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Overlay Steam
        <select value={overrides.steamOverlay ?? "inherit"} onchange={(event) => setSteamOverlay(event.currentTarget.value)} class="h-10 rounded-lg bg-white/5 px-3 text-sm text-zinc-100 light:bg-white light:text-zinc-900">
          <option value="inherit">Eredita default</option>
          <option value="runner_default">Predefinito del runner</option>
          <option value="enabled">Attivo</option>
          <option value="disabled">Disattivo</option>
        </select>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Renderer grafico
        <select value={overrides.graphicsRenderer ?? "inherit"} onchange={(event) => setGraphicsRenderer(event.currentTarget.value)} class="h-10 rounded-lg bg-white/5 px-3 text-sm text-zinc-100 light:bg-white light:text-zinc-900">
          <option value="inherit">Eredita default</option>
          <option value="runner_default">Predefinito del runner</option>
          <option value="wine_d3d">WineD3D</option>
        </select>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Wayland
        <select value={overrides.wayland ?? "inherit"} onchange={(event) => setWayland(event.currentTarget.value)} class="h-10 rounded-lg bg-white/5 px-3 text-sm text-zinc-100 light:bg-white light:text-zinc-900">
          <option value="inherit">Eredita default</option>
          <option value="runner_default">Predefinito del runner</option>
          <option value="disabled">Disattivato</option>
          <option value="native">Nativo, solo GE-Proton</option>
        </select>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Log di debug
        <select value={overrides.debugLogging === null ? "inherit" : String(overrides.debugLogging)} onchange={(event) => setDebugLogging(event.currentTarget.value)} class="h-10 rounded-lg bg-white/5 px-3 text-sm text-zinc-100 light:bg-white light:text-zinc-900">
          <option value="inherit">Eredita default</option>
          <option value="true">Attivi</option>
          <option value="false">Disattivi</option>
        </select>
      </label>
    </div>

    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Impostazioni salvate.</p>{/if}
    <div class="flex flex-wrap gap-2">
      <Button label={saving ? "Salvataggio..." : "Salva impostazioni"} disabled={saving} onClick={() => void saveCompatibility()} />
      <Button label="Ripristina default globali" variant="secondary" disabled={saving} onClick={() => void resetCompatibility()} />
    </div>
  {:else if $appInfo.data.platform === "windows"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">
      Gli argomenti sono passati come singoli parametri al processo. Inserisci un argomento per riga.
    </p>
    <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
      Argomenti di avvio
      <textarea bind:value={nativeArguments} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
      <span class="text-xs text-zinc-500">Nessuna interpretazione shell. Le righe vuote rappresentano argomenti vuoti.</span>
    </label>
    <TextField
      id="native-working-directory"
      label="Cartella di lavoro"
      value={nativeWorkingDirectory}
      placeholder="Cartella dell'eseguibile"
      oninput={(value) => (nativeWorkingDirectory = value)}
    />
    {#if saveError !== null}<ErrorBanner message={saveError} />{/if}
    {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Impostazioni salvate.</p>{/if}
    <div><Button label={saving ? "Salvataggio..." : "Salva impostazioni"} disabled={saving} onClick={() => void saveNative()} /></div>
  {:else}
    <p class="text-sm text-zinc-400 light:text-zinc-600">Le impostazioni di avvio manuale non sono disponibili su questa piattaforma.</p>
  {/if}
</Panel>
