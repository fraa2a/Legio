<script lang="ts">
  import { onMount } from "svelte";
  import { getAppInfo, type AppInfo } from "./lib/services/app";
  import CatalogVerification from "./lib/components/CatalogVerification.svelte";
  import SteamDetailsVerification from "./lib/components/SteamDetailsVerification.svelte";
  import SteamAccountPreference from "./lib/components/SteamAccountPreference.svelte";
  import { cancelGameLaunch, inspectSteamGameLaunch, launchSteamGame, listGameLaunchStates, stopGame, type GameLaunchState, type SteamGameLaunchInspection } from "./lib/services/steam-accounts";
  import {
    createGame,
    getSettings,
    listGames,
    removeGame,
    saveSettings,
    updateGame,
    type Game,
    type Settings,
    type Theme,
  } from "./lib/services/local-state";
  import { importSteamInstallations, scanSteamInstallations, type SteamImportResult, type SteamScan } from "./lib/services/steam";
  import { checkSteamConnectivity, getNetworkLogStatus, getNetworkStatus, type NetworkLogStatus, type NetworkStatus } from "./lib/services/network";
  type LoadState = "loading" | "ready" | "error";

  let appInfo = $state<AppInfo | null>(null);
  let appInfoState = $state<LoadState>("loading");
  let localState = $state<LoadState>("loading");
  let settings = $state<Settings>({ theme: "system" });
  let savedTheme = $state<Theme>("system");
  let games = $state<Game[]>([]);
  let checkingGameIds = $state<string[]>([]);
  let pendingLaunchIds = $state<string[]>([]);
  let cancellingGameIds = $state<string[]>([]);
  let busyGameIds = $state<string[]>([]);
  let launchStates = $state<Record<string, GameLaunchState["status"]>>({});
  let gameLaunchErrors = $state<Record<string, string>>({});
  let launchStateError = $state<string | null>(null);
  let refreshingLaunchStates = false;
  let switchingAccount = $state(false);
  let launchPrompt = $state<{ game: Game; inspection: SteamGameLaunchInspection } | null>(null);
  let launchDialog: HTMLDialogElement;
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let nameDraft = $state("");
  let steamIdDraft = $state("");
  let steamScan = $state<SteamScan | null>(null);
  let scanningSteam = $state(false);
  let steamError = $state<string | null>(null);
  let steamImport = $state<SteamImportResult | null>(null);
  let importingSteam = $state(false);
  let steamImportError = $state<string | null>(null);
  let networkStatus = $state<NetworkStatus | null>(null);
  let networkState = $state<LoadState>("loading");
  let checkingConnectivity = $state(false);
  let networkError = $state<string | null>(null);
  let networkDetail = $state<string | null>(null);
  let networkLogStatus = $state<NetworkLogStatus | null>(null);
  let networkLogState = $state<LoadState>("loading");
  let networkLogError = $state<string | null>(null);
  let systemPrefersDark = $state(window.matchMedia("(prefers-color-scheme: dark)").matches);
  const darkTheme = $derived(settings.theme === "dark" || (settings.theme === "system" && systemPrefersDark));

  function asSteamAppId(value: string): number | null {
    if (!value.trim()) return null;
    const id = Number(value);
    if (!Number.isSafeInteger(id) || id <= 0 || id > 4_294_967_295) {
      throw new Error("Steam App ID must be a positive whole number.");
    }
    return id;
  }

  function messageFor(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  function gameLaunchStatus(gameId: string): "launching" | "running" | null {
    const status = launchStates[gameId];
    if (status === "launching" || status === "running") return status;
    return pendingLaunchIds.includes(gameId) ? "launching" : null;
  }

  async function refreshLaunchStates() {
    if (refreshingLaunchStates) return;
    refreshingLaunchStates = true;
    try {
      const states = await listGameLaunchStates();
      launchStates = Object.fromEntries(states.map((state) => [state.gameId, state.status]));
      gameLaunchErrors = Object.fromEntries(states.filter((state) => state.error).map((state) => [state.gameId, state.error!]));
      cancellingGameIds = cancellingGameIds.filter((id) => pendingLaunchIds.includes(id) || launchStates[id] === "launching");
      launchStateError = null;
    } catch (reason) {
      launchStateError = messageFor(reason);
    } finally {
      refreshingLaunchStates = false;
    }
  }

  async function submitGameLaunch(game: Game, confirmAccountSwitch: boolean) {
    gameLaunchErrors = Object.fromEntries(Object.entries(gameLaunchErrors).filter(([id]) => id !== game.id));
    pendingLaunchIds = [...pendingLaunchIds, game.id];
    try {
      await launchSteamGame(game.id, confirmAccountSwitch);
      message = `Launching ${game.name}.`;
    } finally {
      pendingLaunchIds = pendingLaunchIds.filter((id) => id !== game.id);
      await refreshLaunchStates();
    }
  }

  async function launchGame(game: Game) {
    if (gameLaunchStatus(game.id) || checkingGameIds.includes(game.id)) return;
    checkingGameIds = [...checkingGameIds, game.id];
    error = null;
    message = null;
    try {
      const inspection = await inspectSteamGameLaunch(game.id);
      if (inspection.steamRunning && (inspection.status === "mismatch" || inspection.status === "unknown")) {
        launchPrompt = { game, inspection };
        launchDialog.showModal();
        return;
      }
      await submitGameLaunch(game, false);
    } catch (reason) {
      error = messageFor(reason);
    } finally {
      checkingGameIds = checkingGameIds.filter((id) => id !== game.id);
    }
  }

  function cancelAccountSwitch() {
    if (switchingAccount) return;
    launchDialog.close();
    launchPrompt = null;
  }

  async function confirmAccountSwitch() {
    if (!launchPrompt) return;
    const { game } = launchPrompt;
    launchDialog.close();
    launchPrompt = null;
    switchingAccount = true;
    error = null;
    try {
      await submitGameLaunch(game, true);
    } catch (reason) {
      error = messageFor(reason);
    } finally {
      switchingAccount = false;
    }
  }

  async function endGame(game: Game, status: "launching" | "running") {
    if (busyGameIds.includes(game.id) || cancellingGameIds.includes(game.id)) return;
    if (status === "launching") cancellingGameIds = [...cancellingGameIds, game.id];
    else busyGameIds = [...busyGameIds, game.id];
    error = null;
    message = null;
    try {
      if (status === "launching") {
        await cancelGameLaunch(game.id);
      } else {
        await stopGame(game.id);
        message = `Stopped ${game.name}.`;
      }
      await refreshLaunchStates();
    } catch (reason) {
      cancellingGameIds = cancellingGameIds.filter((id) => id !== game.id);
      error = messageFor(reason);
    } finally {
      busyGameIds = busyGameIds.filter((id) => id !== game.id);
    }
  }

  const launchPromptTitle = $derived(launchPrompt?.inspection.status === "mismatch" ? "Switch Steam account?" : "Steam account could not be verified");
  const launchPromptDescription = $derived(launchPrompt?.inspection.status === "mismatch"
    ? `Steam is open as ${launchPrompt.inspection.currentAccountName ?? "another account"}, but ${launchPrompt.game.name} is set to use ${launchPrompt.inspection.targetAccountName ?? "the selected account"}. Continuing will close Steam and any running Steam games, then reopen Steam with the selected account.`
    : `Steam is running, but Legio could not determine which account it is using for ${launchPrompt?.game.name ?? "this game"}. Continuing will close Steam and any running Steam games, then reopen Steam with the selected account.`);

  async function loadAppInfo() {
    appInfoState = "loading";
    try {
      appInfo = await getAppInfo();
      appInfoState = "ready";
    } catch {
      appInfoState = "error";
    }
  }

  async function loadLocalState() {
    localState = "loading";
    error = null;
    message = null;
    try {
      const [loadedSettings, loadedGames] = await Promise.all([getSettings(), listGames()]);
      settings = { ...loadedSettings };
      savedTheme = loadedSettings.theme;
      games = loadedGames;
      localState = "ready";
    } catch (reason) {
      error = messageFor(reason);
      localState = "error";
    }
  }

  async function persistSettings() {
    error = null;
    message = null;
    try {
      const saved = await saveSettings(settings);
      settings = { ...saved };
      savedTheme = saved.theme;
      message = "Theme setting saved.";
    } catch (reason) {
      error = messageFor(reason);
    }
  }

  async function addGame() {
    error = null;
    message = null;
    try {
      const game = await createGame({ name: nameDraft, steamAppId: asSteamAppId(steamIdDraft) });
      games = [...games, game];
      nameDraft = "";
      steamIdDraft = "";
      message = "Created local entry.";
    } catch (reason) {
      error = messageFor(reason);
    }
  }

  async function persistGame(game: Game, form: HTMLFormElement) {
    const fields = new FormData(form);
    error = null;
    message = null;
    try {
      const updated = await updateGame({
        id: game.id,
        steamAppId: asSteamAppId(String(fields.get("steamAppId") ?? "")),
        automaticName: String(fields.get("automaticName") ?? "").trim() || null,
        nameOverride: String(fields.get("nameOverride") ?? "").trim() || null,
      });
      games = games.map((entry) => entry.id === updated.id ? updated : entry);
      message = "Local entry updated.";
    } catch (reason) {
      error = messageFor(reason);
    }
  }

  async function deleteGame(game: Game) {
    error = null;
    message = null;
    try {
      await removeGame(game.id);
      games = games.filter((entry) => entry.id !== game.id);
      message = "Local entry removed.";
    } catch (reason) {
      error = messageFor(reason);
    }
  }

  async function scanSteam() {
    scanningSteam = true;
    steamError = null;
    steamScan = null;
    try {
      steamScan = await scanSteamInstallations();
    } catch (reason) {
      steamError = messageFor(reason);
    } finally {
      scanningSteam = false;
    }
  }

  async function importSteam() {
    importingSteam = true;
    steamImportError = null;
    steamImport = null;
    steamScan = null;
    try {
      steamImport = await importSteamInstallations();
      try {
        games = await listGames();
      } catch (reason) {
        error = `Import committed, but the library could not reload: ${messageFor(reason)}`;
      }
    } catch (reason) {
      steamImportError = messageFor(reason);
    } finally {
      importingSteam = false;
    }
  }

  async function loadNetworkStatus() {
    networkState = "loading";
    networkError = null;
    networkDetail = null;
    networkStatus = null;
    try {
      networkStatus = await getNetworkStatus();
      networkState = "ready";
    } catch (reason) {
      networkError = messageFor(reason);
      networkState = "error";
    }
  }

  async function checkConnectivity() {
    checkingConnectivity = true;
    networkError = null;
    networkDetail = null;
    networkStatus = null;
    try {
      const result = await checkSteamConnectivity();
      networkStatus = result.status;
      networkDetail = result.detail;
      networkState = "ready";
    } catch (reason) {
      networkError = messageFor(reason);
      networkState = "error";
    } finally {
      checkingConnectivity = false;
      void loadNetworkLogStatus();
    }
  }

  async function loadNetworkLogStatus() {
    networkLogState = "loading";
    networkLogError = null;
    try {
      networkLogStatus = await getNetworkLogStatus();
      networkLogState = "ready";
    } catch (reason) {
      networkLogError = messageFor(reason);
      networkLogState = "error";
    }
  }

  $effect(() => {
    void loadAppInfo();
    void loadLocalState().then(() => {
      if (localState === "ready") void importSteam();
    });
    void loadNetworkStatus();
    void loadNetworkLogStatus();
  });

  onMount(() => {
    void refreshLaunchStates();
    const timer = window.setInterval(() => void refreshLaunchStates(), 1000);
    return () => window.clearInterval(timer);
  });

  $effect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => { systemPrefersDark = query.matches; };
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  });
</script>

<svelte:head>
  <title>Legio backend verification</title>
</svelte:head>

<main class={darkTheme ? "min-h-screen bg-slate-950 px-6 py-12 text-slate-100" : "min-h-screen bg-slate-100 px-6 py-12 text-slate-950"}>
  <section class={darkTheme ? "mx-auto w-full max-w-4xl rounded-2xl border border-slate-800 bg-slate-900 p-8" : "mx-auto w-full max-w-4xl rounded-2xl border border-slate-300 bg-white p-8"}>
    <p class="text-sm font-semibold tracking-[0.2em] text-amber-400 uppercase">Legio</p>
    <h1 class="mt-3 text-3xl font-semibold">Backend verification</h1>
    <p class="mt-3 text-sm text-slate-400">
      Functional local state, Steam detection, catalog search, Steam details, and connectivity testing. This is not the final product UI.
    </p>

    {#if appInfoState === "loading"}
      <p class="mt-6 text-sm" role="status">Loading native application information...</p>
    {:else if appInfoState === "error"}
      <div class="mt-6 rounded-xl border border-red-900 bg-red-950/30 p-4" role="alert">
        <p>Native backend unavailable.</p>
        <button class="mt-3 underline" type="button" onclick={loadAppInfo}>Retry application information</button>
      </div>
    {:else if appInfo}
      <dl class="mt-6 grid gap-3 sm:grid-cols-3">
        <div class="rounded border border-slate-800 p-3"><dt class="text-xs text-slate-500">Name</dt><dd>{appInfo.name}</dd></div>
        <div class="rounded border border-slate-800 p-3"><dt class="text-xs text-slate-500">Version</dt><dd>{appInfo.version}</dd></div>
        <div class="rounded border border-slate-800 p-3"><dt class="text-xs text-slate-500">Platform</dt><dd>{appInfo.platform}</dd></div>
      </dl>
    {/if}

    {#if localState === "loading"}
      <p class="mt-8 text-sm" role="status">Loading persisted local state...</p>
    {:else if localState === "error"}
      <div class="mt-8 rounded-xl border border-red-900 bg-red-950/30 p-4" role="alert">
        <p class="break-words">{error}</p>
        <button class="mt-3 underline" type="button" onclick={loadLocalState}>Retry local state</button>
      </div>
    {:else}
      <section class="mt-8" aria-labelledby="settings-heading">
        <div class="flex items-baseline justify-between gap-4">
          <h2 id="settings-heading" class="text-xl font-semibold">Settings</h2>
          <button class="text-sm underline" type="button" onclick={loadLocalState}>Reload saved state</button>
        </div>
        <div class="mt-4 rounded border border-slate-800 p-4">
          <label class="block text-sm" for="theme">Theme</label>
          <select id="theme" class="mt-2 rounded border border-slate-600 bg-slate-950 px-3 py-2" bind:value={settings.theme}>
            <option value="system">System</option>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
          <p class="mt-2 text-xs text-slate-500">Saved theme: {savedTheme}</p>
          <button class="mt-4 rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950" type="button" onclick={persistSettings}>Save settings</button>
        </div>
      </section>

      <section class="mt-8" aria-labelledby="library-heading">
        <h2 id="library-heading" class="text-xl font-semibold">Local library</h2>
        <p class="mt-2 text-sm text-slate-400">Manual entries and imported Steam metadata. Saved installation paths reflect the last successful import, not current launch readiness.</p>
        <form class="mt-4 grid gap-3 sm:grid-cols-3" onsubmit={(event) => { event.preventDefault(); void addGame(); }}>
          <label class="text-sm">Initial name<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" required bind:value={nameDraft} /></label>
          <label class="text-sm">Steam App ID (optional)<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" inputmode="numeric" bind:value={steamIdDraft} /></label>
          <button class="self-end rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950" type="submit">Create local entry</button>
        </form>

        {#if games.length === 0}
          <p class="mt-4 rounded border border-dashed border-slate-600 p-4 text-sm text-slate-400">No local library entries yet.</p>
        {:else}
          <div class="mt-4 space-y-4">
            {#each games as game (game.id)}
              {@const status = gameLaunchStatus(game.id)}
              <form class="rounded border border-slate-800 p-4" onsubmit={(event) => { event.preventDefault(); void persistGame(game, event.currentTarget); }}>
                <div class="flex items-baseline justify-between gap-4">
                  <h3 class="font-semibold">{game.name}</h3>
                  <button class="text-sm text-red-300 underline" type="button" onclick={() => void deleteGame(game)}>Remove entry</button>
                </div>
                <p class="mt-2 break-all text-xs text-slate-500">Internal ID: {game.id}</p>
                {#if game.steamInstallPath}
                  <p class="mt-2 break-all text-xs text-slate-400">Steam installation: {game.steamInstallPath}</p>
                {/if}
                <div class="mt-4 grid gap-3 sm:grid-cols-3">
                  <label class="text-sm">Steam App ID<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" name="steamAppId" value={game.steamAppId ?? ""} /></label>
                  <label class="text-sm">Automatic name<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" name="automaticName" value={game.automaticName ?? ""} /></label>
                  <label class="text-sm">Name override<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" name="nameOverride" value={game.nameOverride ?? ""} /></label>
                </div>
                <p class="mt-3 text-xs text-slate-500">Manual name overrides take precedence over automatic names.</p>
                <button class="mt-4 rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950" type="submit">Save entry</button>
                {#if status}
                  <span class="ml-3 text-sm text-slate-300" role="status">{status === "launching" ? "Launching" : "Running"}</span>
                  <button class="ml-3 mt-4 rounded border border-slate-600 px-3 py-2 text-sm disabled:opacity-50" type="button" disabled={busyGameIds.includes(game.id) || cancellingGameIds.includes(game.id)} onclick={() => void endGame(game, status)}>{cancellingGameIds.includes(game.id) ? "Cancelling..." : busyGameIds.includes(game.id) ? "Stopping..." : status === "launching" ? "Cancel" : "Stop"}</button>
                {:else}
                  <button class="ml-3 mt-4 rounded border border-slate-600 px-3 py-2 text-sm disabled:opacity-50" type="button" disabled={checkingGameIds.includes(game.id) || !game.steamAppId || !game.steamInstallPath} onclick={() => void launchGame(game)}>{checkingGameIds.includes(game.id) ? "Checking Steam..." : "Play"}</button>
                {/if}
                {#if gameLaunchErrors[game.id]}
                  <p class="mt-3 text-sm text-red-300" role="alert">Launch failed: {gameLaunchErrors[game.id]}</p>
                {/if}
                <SteamAccountPreference {game} onSaved={(updated) => { games = games.map((entry) => entry.id === updated.id ? updated : entry); }} />
              </form>
            {/each}
          </div>
        {/if}
        {#if launchStateError}
          <p class="mt-3 text-sm text-red-300" role="alert">Game status unavailable: {launchStateError}</p>
        {/if}
      </section>
    {/if}

    <section class="mt-8" aria-labelledby="steam-heading">
      <h2 id="steam-heading" class="text-xl font-semibold">Local Steam detection and import</h2>
      <p class="mt-2 text-sm text-slate-400">Installed Steam games are imported when Legio opens. Rescan refreshes automatic metadata and preserves manual names. Missing games are retained; previously imported Steam tools are removed only when unchanged. Local data is not sent remotely.</p>
      <div class="mt-4 flex flex-wrap gap-3">
        <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={scanningSteam || importingSteam} onclick={scanSteam}>
          {scanningSteam ? "Scanning..." : "Scan local Steam installations"}
        </button>
        <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={scanningSteam || importingSteam || localState !== "ready"} onclick={importSteam}>
          {importingSteam ? "Importing..." : "Rescan and import Steam games"}
        </button>
      </div>
      {#if importingSteam}
        <p class="mt-3 text-sm" role="status">Scanning and saving installed Steam games...</p>
      {:else if steamImportError}
        <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">Steam import failed: {steamImportError}. No import changes were committed.</p>
      {/if}
      {#if steamImport}
        <p class="mt-3 text-sm" role="status">Import committed. Detected {steamImport.detected} Steam games. Library rows: {steamImport.inserted} inserted, {steamImport.updated} updated, {steamImport.unchanged} unchanged, {steamImport.removed} unchanged Steam tools removed.</p>
        <p class="mt-2 text-xs text-slate-400">Existing entries sharing a Steam App ID are refreshed separately, never merged. Nothing was launched.</p>
        {#if steamImport.diagnostics.length > 0}
          <ul class="mt-3 list-disc space-y-1 pl-5 text-xs text-slate-400">
            {#each steamImport.diagnostics as diagnostic, index (index)}<li class="break-words">{diagnostic}</li>{/each}
          </ul>
        {/if}
      {/if}
      {#if scanningSteam}
        <p class="mt-3 text-sm" role="status">Reading local Steam installation metadata...</p>
      {:else if steamError}
        <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">Steam scan failed: {steamError}. Run the native app and retry the scan.</p>
      {/if}
      {#if steamScan}
        <p class="mt-3 text-sm" role="status">Scan completed. Detected {steamScan.games.length} Steam game record{steamScan.games.length === 1 ? "" : "s"}. This scan did not change the library or launch games.</p>
        {#if steamScan.games.length > 0}
          <ul class="mt-3 space-y-2 text-sm">
            {#each steamScan.games as game (game.appId)}
              <li class="rounded border border-slate-800 p-3">
                <p>{game.name} (Steam App ID {game.appId})</p>
                <p class="mt-1 break-all text-xs text-slate-400">Installation directory: {game.installPath}</p>
              </li>
            {/each}
          </ul>
        {/if}
        {#if steamScan.diagnostics.length > 0}
          <ul class="mt-3 list-disc space-y-1 pl-5 text-xs text-slate-400">
            {#each steamScan.diagnostics as diagnostic, index (index)}<li class="break-words">{diagnostic}</li>{/each}
          </ul>
        {/if}
      {/if}
    </section>

    <CatalogVerification />
    <SteamDetailsVerification />

    <section class="mt-8" aria-labelledby="network-heading">
      <h2 id="network-heading" class="text-xl font-semibold">Steam connectivity</h2>
      <p class="mt-2 text-sm text-slate-400">Reads Rust-owned connectivity state. Checking contacts the public Steam Store; it does not fetch the catalog or send local game data.</p>
      <div class="mt-4 flex flex-wrap gap-3">
        <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={networkState === "loading" || checkingConnectivity} onclick={checkConnectivity}>
          {checkingConnectivity ? "Checking..." : "Check Steam connectivity"}
        </button>
        <button class="text-sm underline disabled:opacity-50" type="button" disabled={networkState === "loading" || checkingConnectivity} onclick={loadNetworkStatus}>Reload network status</button>
      </div>
      {#if checkingConnectivity}
        <p class="mt-3 text-sm" role="status">Contacting Steam Store...</p>
      {:else if networkState === "loading"}
        <p class="mt-3 text-sm" role="status">Loading native network status...</p>
      {:else if networkError}
        <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">Connectivity command failed: {networkError}. Run the native app and retry.</p>
      {:else if networkStatus}
        <p class="mt-3 text-sm" role="status">Native network status: {networkStatus === "online" ? "Online" : "Unknown"}.</p>
        <p class="mt-2 text-xs text-slate-400">{networkStatus === "online" ? "Steam Store responded to the last check. This does not guarantee catalog or download availability." : "Connectivity has not been established. Unknown does not mean that the device is offline."}</p>
      {/if}
      {#if networkDetail}
        <p class="mt-3 break-words rounded border border-amber-800 p-4 text-sm" role="alert">{networkDetail}</p>
      {/if}
    </section>

    <section class="mt-8" aria-labelledby="network-log-heading">
      <div class="flex items-baseline justify-between gap-4">
        <h2 id="network-log-heading" class="text-xl font-semibold">Network diagnostics</h2>
        <button class="text-sm underline disabled:opacity-50" type="button" disabled={networkLogState === "loading"} onclick={loadNetworkLogStatus}>Reload log status</button>
      </div>
      <p class="mt-2 text-sm text-slate-400">Local request outcomes for Steam and Hydra. Logs contain no URLs, request content, or game data.</p>
      {#if networkLogState === "loading"}
        <p class="mt-3 text-sm" role="status">Loading local log status...</p>
      {:else if networkLogError}
        <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">Could not read log status: {networkLogError}. Retry above.</p>
      {:else if networkLogStatus}
        {#if networkLogStatus.lastError}
          <p class="mt-3 break-words rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">{networkLogStatus.lastError}</p>
        {:else}
          <p class="mt-3 text-sm" role="status">Local logging is available.</p>
        {/if}
        {#if networkLogStatus.directory}
          <p class="mt-2 break-all text-xs text-slate-400">Log directory: {networkLogStatus.directory}</p>
        {/if}
        <p class="mt-2 text-xs text-slate-400">Pending records: {networkLogStatus.pendingRecords}. Dropped records: {networkLogStatus.droppedRecords}.</p>
      {/if}
    </section>

    {#if error && localState === "ready"}
      <p class="mt-6 rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">{error}</p>
    {/if}
    {#if message}
      <p class="mt-6 rounded border border-emerald-800 bg-emerald-950/30 p-4 text-sm" role="status">{message}</p>
    {/if}
  </section>
</main>
<dialog bind:this={launchDialog} oncancel={(event) => { event.preventDefault(); cancelAccountSwitch(); }} class="w-[min(32rem,calc(100%-2rem))] rounded-xl border border-slate-700 bg-slate-900 p-6 text-slate-100 backdrop:bg-black/70">
  <h2 class="text-lg font-semibold" id="launch-prompt-title">{launchPromptTitle}</h2>
  <p class="mt-3 text-sm text-slate-300" id="launch-prompt-description">{launchPromptDescription}</p>
  <p class="mt-3 text-xs text-slate-400">Steam may ask you to sign in or complete Steam Guard if the saved session is no longer valid.</p>
  <div class="mt-5 flex justify-end gap-3">
    <button class="rounded border border-slate-600 px-3 py-2 text-sm disabled:opacity-50" type="button" disabled={switchingAccount} onclick={cancelAccountSwitch}>Cancel</button>
    <button class="rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950 disabled:opacity-50" type="button" disabled={switchingAccount} onclick={() => void confirmAccountSwitch()}>{switchingAccount ? "Switching account..." : "Continue and switch"}</button>
  </div>
</dialog>
