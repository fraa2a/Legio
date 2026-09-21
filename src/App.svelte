<script lang="ts">
  import { getAppInfo, type AppInfo } from "./lib/services/app";

  type LoadState = "loading" | "ready" | "error";

  let appInfo = $state<AppInfo | null>(null);
  let loadState = $state<LoadState>("loading");

  async function loadAppInfo() {
    loadState = "loading";
    appInfo = null;

    try {
      appInfo = await getAppInfo();
      loadState = "ready";
    } catch {
      loadState = "error";
    }
  }

  $effect(() => {
    void loadAppInfo();
  });
</script>

<svelte:head>
  <title>Legio</title>
</svelte:head>

<main class="flex min-h-screen items-center justify-center bg-slate-950 px-6 py-12 text-slate-100">
  <section class="w-full max-w-xl rounded-2xl border border-slate-800 bg-slate-900 p-8 shadow-2xl shadow-black/30">
    <p class="text-sm font-semibold tracking-[0.2em] text-amber-400 uppercase">Legio</p>
    <h1 class="mt-3 text-3xl font-semibold tracking-tight">Application information</h1>
    <p class="mt-3 text-sm leading-6 text-slate-400">
      This foundation screen reads its identity directly from the native Rust application.
    </p>

    {#if loadState === "loading"}
      <div class="mt-8 rounded-xl border border-slate-800 bg-slate-950/60 p-5" role="status">
        <p class="text-sm text-slate-300">Loading native application information...</p>
      </div>
    {:else if loadState === "error"}
      <div class="mt-8 rounded-xl border border-red-900/70 bg-red-950/30 p-5" role="alert">
        <h2 class="font-medium text-red-200">Native backend unavailable</h2>
        <p class="mt-2 text-sm leading-6 text-red-200/70">
          Legio could not reach its Rust backend. Run this interface through the Tauri desktop application and try again.
        </p>
        <button
          class="mt-5 rounded-lg bg-red-200 px-4 py-2 text-sm font-semibold text-red-950 transition hover:bg-red-100 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-red-200"
          type="button"
          onclick={loadAppInfo}
        >
          Retry
        </button>
      </div>
    {:else if appInfo}
      <dl class="mt-8 grid gap-4 sm:grid-cols-3" aria-label="Application details">
        <div class="rounded-xl border border-slate-800 bg-slate-950/60 p-4">
          <dt class="text-xs font-medium tracking-wide text-slate-500 uppercase">Name</dt>
          <dd class="mt-2 font-medium text-slate-100">{appInfo.name}</dd>
        </div>
        <div class="rounded-xl border border-slate-800 bg-slate-950/60 p-4">
          <dt class="text-xs font-medium tracking-wide text-slate-500 uppercase">Version</dt>
          <dd class="mt-2 font-medium text-slate-100">{appInfo.version}</dd>
        </div>
        <div class="rounded-xl border border-slate-800 bg-slate-950/60 p-4">
          <dt class="text-xs font-medium tracking-wide text-slate-500 uppercase">Platform</dt>
          <dd class="mt-2 font-medium text-slate-100">{appInfo.platform}</dd>
        </div>
      </dl>
    {/if}
  </section>
</main>
