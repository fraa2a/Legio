<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import {
    emptyCompatibilityDefaults,
    getCompatibilityDefaults,
    getCompatibilityLogsDirectory,
    listCompatibilityRunners,
    saveCompatibilityDefaults,
    type CompatibilityDefaults,
    type GraphicsRenderer,
    type WaylandMode,
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
    for (const key of Object.keys(environment)) {
      if (typedKeys.some((managed) => key.toLowerCase() === managed.toLowerCase())) {
        throw new Error(`La variabile ${key} è gestita dalle opzioni tipizzate di Legio.`);
      }
      if (defaults.debugLogging && ["PROTON_LOG", "PROTON_LOG_DIR", "SteamGameId"].some((managed) => key.toLowerCase() === managed.toLowerCase())) {
        throw new Error(`La variabile ${key} è gestita dai log di debug.`);
      }
    }
    return environment;
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
        environment: parseEnvironment(environmentText),
        dllOverrides: parseMap(dllOverridesText, "Override DLL"),
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
    { value: "", label: "Scelta automatica" },
    ...(defaults.runnerPath && !runners.some((runner) => runner.path === defaults.runnerPath)
      ? [{ value: defaults.runnerPath, label: defaults.runnerPath }]
      : []),
    ...runners.map((runner) => ({
      value: runner.path,
      label: runner.kind === "wine" ? `${runner.name} (${runner.version})` : runner.name,
    })),
  ]);

  function setGraphicsRenderer(value: string): void {
    defaults = { ...defaults, graphicsRenderer: value as GraphicsRenderer };
  }

  function setWayland(value: string): void {
    defaults = { ...defaults, wayland: value as WaylandMode };
  }
</script>

<section class="flex flex-col gap-5">
  <div>
    <h3 class="text-xl font-semibold text-zinc-100 light:text-zinc-900">Compatibilità</h3>
    <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">
      Valori predefiniti per l'avvio dei giochi Windows tramite runner Linux. I giochi usano questi valori se non hanno override propri.
    </p>
  </div>

  {#if loading}
    <p class="text-sm text-zinc-400" role="status">Caricamento dei runner e dei default...</p>
  {:else if $appInfo.status === "error"}
    <ErrorBanner message={$appInfo.error ?? "Impossibile rilevare la piattaforma."} onRetry={() => void load()} />
  {:else if $appInfo.data.platform !== "linux"}
    <p class="rounded-lg bg-white/5 p-4 text-sm text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
      I runner e i default di compatibilità sono disponibili su Linux. Le impostazioni di avvio native si configurano nella pagina di ogni gioco Windows.
    </p>
  {:else}
    {#if loadError !== null}
      <ErrorBanner message={loadError} onRetry={() => void load()} />
    {/if}
    {#if saveError !== null}
      <ErrorBanner message={saveError} />
    {/if}
    {#if saved}
      <p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Default salvati.</p>
    {/if}

    {#if runnerDiagnostics.length > 0}
      <div class="rounded-lg bg-amber-500/10 p-3 text-sm text-amber-200 light:text-amber-900">
        <p class="font-medium">Rilevamento runner</p>
        <ul class="mt-2 list-disc pl-5">
          {#each runnerDiagnostics as diagnostic (diagnostic)}
            <li>{diagnostic}</li>
          {/each}
        </ul>
      </div>
    {/if}
    {#if runners.length === 0}
      <p class="text-sm text-zinc-400 light:text-zinc-600">Nessun runner compatibile rilevato.</p>
    {/if}

    <div class="grid gap-4 md:grid-cols-2">
      <SelectField id="compat-runner" label="Runner predefinito" value={defaults.runnerPath ?? ""} options={runnerOptions} onChange={(value) => (defaults = { ...defaults, runnerPath: value || null })} />
      <TextField
        id="compat-prefix-root"
        label="Cartella predefinita dei prefix"
        value={defaults.prefixRoot ?? ""}
        placeholder="Percorso opzionale"
        hint="Legio crea un prefix per gioco dentro questa cartella."
        oninput={(value) => (defaults = { ...defaults, prefixRoot: value || null })}
      />
      <label class="flex items-center gap-2 text-sm text-zinc-300 light:text-zinc-700">
        <input
          type="checkbox"
          class="size-4 accent-white"
          checked={defaults.debugLogging}
          onchange={(event) => (defaults = { ...defaults, debugLogging: event.currentTarget.checked })}
        />
        Abilita log di debug per gli avvii
      </label>
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <SelectField id="compat-renderer" label="Renderer grafico" value={defaults.graphicsRenderer} options={[{ value: "runner_default", label: "Predefinito del runner" }, { value: "wine_d3d", label: "WineD3D" }]} onChange={setGraphicsRenderer} />
      <SelectField id="compat-wayland" label="Wayland" value={defaults.wayland} options={[{ value: "runner_default", label: "Predefinito del runner" }, { value: "disabled", label: "Disattivato" }, { value: "native", label: "Nativo, solo GE-Proton" }]} onChange={setWayland} />
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Argomenti prima dell'eseguibile
        <textarea bind:value={argumentsBefore} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Un argomento per riga. Le righe vuote rappresentano argomenti vuoti.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Argomenti dopo l'eseguibile
        <textarea bind:value={argumentsAfter} rows="4" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Un argomento per riga, passato senza interpretazione shell.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Variabili ambiente
        <textarea bind:value={environmentText} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Una voce KEY=VALUE per riga. I controlli tipizzati hanno priorità.</span>
      </label>
      <label class="flex flex-col gap-1.5 text-sm text-zinc-400 light:text-zinc-600">
        Override DLL
        <textarea bind:value={dllOverridesText} rows="5" class="rounded-lg bg-white/5 p-3 font-mono text-sm text-zinc-100 light:bg-white light:text-zinc-900"></textarea>
        <span class="text-xs text-zinc-500">Una voce KEY=VALUE per riga, ad esempio d3d11=n,b.</span>
      </label>
    </div>

    {#if logsDirectory !== null}
      <p class="break-all text-xs text-zinc-500">Log compatibilità: {logsDirectory}</p>
    {/if}
    <div>
      <Button label={saving ? "Salvataggio..." : "Salva default"} disabled={saving || loading} onClick={() => void save()} />
    </div>
  {/if}
</section>
