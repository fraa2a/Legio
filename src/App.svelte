<script lang="ts">
  import { t, language } from "./lib/i18n";
  import { onMount } from "svelte";
  import Sidebar from "./lib/components/layout/Sidebar.svelte";
  import MainContainer from "./lib/components/layout/MainContainer.svelte";
  import TitleBar from "./lib/components/layout/TitleBar.svelte";
  import SettingsDialog from "./lib/features/settings/SettingsDialog.svelte";
  import { hydrateApp } from "./lib/stores/bootstrap";
  import { closeSettings, settingsOpen } from "./lib/stores/navigation";
  import { resolvedTheme, settings } from "./lib/stores/settings";
  import { checkForAppUpdate, installAppUpdate, updateState } from "./lib/services/app-updater";

  let updateDismissed = $state(false);

  onMount(() => {
    void hydrateApp().catch((reason) => console.error(reason));
    if (import.meta.env.PROD) void checkForAppUpdate();

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      const theme = $settings.data.theme;
      document.documentElement.dataset.theme = resolvedTheme(theme, media.matches);
    };

    applyTheme();
    const unsubscribe = settings.subscribe(applyTheme);
    media.addEventListener("change", applyTheme);

    return () => {
      unsubscribe();
      media.removeEventListener("change", applyTheme);
    };
  });

  function blockContextMenu(event: MouseEvent) {
    event.preventDefault();
  }
</script>

<svelte:head>
  <title>Legio</title>
</svelte:head>

<svelte:window oncontextmenu={blockContextMenu} />

<div class="flex h-dvh select-none bg-black light:bg-zinc-200">
  <Sidebar />
  <div class="flex min-w-0 flex-1 flex-col pl-2.5 pr-2.5 pb-1.5">
    <TitleBar />
    <MainContainer />
  </div>
</div>

{#if $settingsOpen}
  <SettingsDialog onClose={closeSettings} />
{/if}

{#if $updateState.version && !updateDismissed}
  <div class="fixed right-5 bottom-5 z-50 flex max-w-sm items-center gap-3 rounded-xl bg-zinc-800 p-4 text-sm text-white shadow-xl light:bg-white light:text-zinc-900" role="status">
    <span>Legio {$updateState.version}{t(" disponibile.", $language)}{#if $updateState.aur}{t("Aggiorna tramite AUR.", $language)}{/if}</span>
    {#if !$updateState.aur}
      <button type="button" class="rounded-lg bg-white px-3 py-2 text-zinc-900 disabled:opacity-50 light:bg-zinc-900 light:text-white" disabled={$updateState.installing} onclick={() => void installAppUpdate()}>
        {$updateState.installing ? t("Installazione...", $language) : t("Aggiorna", $language)}
      </button>
    {/if}
    <button type="button" class="text-zinc-400 hover:text-white light:hover:text-zinc-900" aria-label={t("Chiudi avviso aggiornamento", $language)} onclick={() => (updateDismissed = true)}>{t("Chiudi", $language)}</button>
  </div>
{/if}
