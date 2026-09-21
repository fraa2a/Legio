<script lang="ts">
  import { getAppInfo, type AppInfo } from "./lib/services/app";
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

  type LoadState = "loading" | "ready" | "error";

  let appInfo = $state<AppInfo | null>(null);
  let appInfoState = $state<LoadState>("loading");
  let localState = $state<LoadState>("loading");
  let settings = $state<Settings>({ theme: "system" });
  let savedTheme = $state<Theme>("system");
  let games = $state<Game[]>([]);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let nameDraft = $state("");
  let steamIdDraft = $state("");
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

  $effect(() => {
    void loadAppInfo();
    void loadLocalState();
  });

  $effect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => { systemPrefersDark = query.matches; };
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  });
</script>

<svelte:head>
  <title>Legio local state verification</title>
</svelte:head>

<main class={darkTheme ? "min-h-screen bg-slate-950 px-6 py-12 text-slate-100" : "min-h-screen bg-slate-100 px-6 py-12 text-slate-950"}>
  <section class={darkTheme ? "mx-auto w-full max-w-4xl rounded-2xl border border-slate-800 bg-slate-900 p-8" : "mx-auto w-full max-w-4xl rounded-2xl border border-slate-300 bg-white p-8"}>
    <p class="text-sm font-semibold tracking-[0.2em] text-amber-400 uppercase">Legio</p>
    <h1 class="mt-3 text-3xl font-semibold">Local state verification</h1>
    <p class="mt-3 text-sm text-slate-400">
      Functional persistence testing only. This is not the final product UI.
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
        <p class="mt-2 text-sm text-slate-400">Entries are local metadata, not installed or launchable games.</p>
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
              <form class="rounded border border-slate-800 p-4" onsubmit={(event) => { event.preventDefault(); void persistGame(game, event.currentTarget); }}>
                <div class="flex items-baseline justify-between gap-4">
                  <h3 class="font-semibold">{game.name}</h3>
                  <button class="text-sm text-red-300 underline" type="button" onclick={() => void deleteGame(game)}>Remove entry</button>
                </div>
                <p class="mt-2 break-all text-xs text-slate-500">Internal ID: {game.id}</p>
                <div class="mt-4 grid gap-3 sm:grid-cols-3">
                  <label class="text-sm">Steam App ID<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" name="steamAppId" value={game.steamAppId ?? ""} /></label>
                  <label class="text-sm">Automatic name<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" name="automaticName" value={game.automaticName ?? ""} /></label>
                  <label class="text-sm">Name override<input class="mt-1 block w-full rounded border border-slate-600 bg-slate-950 px-3 py-2" name="nameOverride" value={game.nameOverride ?? ""} /></label>
                </div>
                <p class="mt-3 text-xs text-slate-500">Manual name overrides take precedence over automatic names.</p>
                <button class="mt-4 rounded bg-amber-400 px-3 py-2 text-sm font-semibold text-slate-950" type="submit">Save entry</button>
              </form>
            {/each}
          </div>
        {/if}
      </section>
    {/if}

    {#if error && localState === "ready"}
      <p class="mt-6 rounded border border-red-900 bg-red-950/30 p-4 text-sm" role="alert">{error}</p>
    {/if}
    {#if message}
      <p class="mt-6 rounded border border-emerald-800 bg-emerald-950/30 p-4 text-sm" role="status">{message}</p>
    {/if}
  </section>
</main>
