<script lang="ts">
  import { t, language } from "./lib/i18n";
  import { activePalette, applyAppearance } from "./lib/services/appearance";
  import { appearancePreview } from "./lib/stores/appearance";
  import { appInfo } from "./lib/stores/app-info";
  import AppBackground from "./lib/components/layout/AppBackground.svelte";
  import { onMount } from "svelte";
  import { derived } from "svelte/store";
  import Sidebar from "./lib/components/layout/Sidebar.svelte";
  import OnboardingView from "./lib/features/onboarding/OnboardingView.svelte";
  import ErrorBanner from "./lib/components/ui/ErrorBanner.svelte";
  import MainContainer from "./lib/components/layout/MainContainer.svelte";
  import TitleBar from "./lib/components/layout/TitleBar.svelte";
  import SettingsDialog from "./lib/features/settings/SettingsDialog.svelte";
  import ToastNotice from "./lib/components/ui/ToastNotice.svelte";
  import { hydrateApp } from "./lib/stores/bootstrap";
  import { closeSettings, settingsOpen } from "./lib/stores/navigation";
  import { settings } from "./lib/stores/settings";
  import { checkForAppUpdate, installAppUpdate, updateState } from "./lib/services/app-updater";
  import { windowActive } from "./lib/stores/window-activity";
  import { hasRunningGame } from "./lib/stores/launch";
  import { observeMediaPlayback } from "./lib/services/media-playback";

  const mediaActive = derived([windowActive, hasRunningGame], ([$active, $running]) => $active && !$running);
  const appearance = $derived($appearancePreview?.appearance ?? $settings.data.appearance);
  const theme = $derived($appearancePreview?.theme ?? $settings.data.theme);
  let prefersDark = $state(window.matchMedia("(prefers-color-scheme: dark)").matches);
  const accent = $derived(activePalette(theme, prefersDark).palette.accent);

  let updateDismissed = $state(false);

  $effect(() => {
    if (!$updateState.version || $appInfo.data.platform !== "linux") return;
    updateDismissed = false;
    const timer = setTimeout(() => { updateDismissed = true; }, 8000);
    return () => clearTimeout(timer);
  });

  onMount(() => {
    const stopMedia = observeMediaPlayback(mediaActive.subscribe);
    void hydrateApp().catch((reason) => console.error(reason));
    if (import.meta.env.PROD) void checkForAppUpdate();

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const updateSystemTheme = () => {
      prefersDark = media.matches;
    };

    media.addEventListener("change", updateSystemTheme);

    return () => {
      stopMedia();
      media.removeEventListener("change", updateSystemTheme);
    };
  });

  $effect(() => {
    applyAppearance(document.documentElement, theme, appearance, prefersDark,
      $appInfo.data.platform === "linux" && $appInfo.data.desktopEnvironment === "hyprland");
  });

  function blockContextMenu(event: MouseEvent) {
    event.preventDefault();
  }
</script>

<svelte:head>
  <title>Legio</title>
</svelte:head>

<svelte:window
  oncontextmenu={blockContextMenu}
  ondragstart={(event) => { if (event.target instanceof HTMLImageElement) event.preventDefault(); }}
  onpointerdown={() => { document.documentElement.dataset.inputMethod = "pointer"; }}
  onkeydown={(event) => { if (event.key === "Tab" || event.key.startsWith("Arrow")) document.documentElement.dataset.inputMethod = "keyboard"; }}
/>

<AppBackground id={appearance.background} animation={appearance.animatedBackground} animationOpacity={appearance.animatedOpacity} {accent} dither={appearance.dither} active={$mediaActive} />
<div class="legio-shell relative flex h-dvh select-none">
  {#if $settings.status === "ready" && $settings.data.onboardingCompleted}<Sidebar />{/if}
  <div class="flex min-w-0 flex-1 flex-col pl-2.5 pr-2.5 pb-1.5">
    <TitleBar />
    {#if $settings.status === "error" || $appInfo.status === "error"}
      <ErrorBanner message={$settings.error ?? $appInfo.error ?? t("Impossibile caricare la configurazione.", $language)} onRetry={() => void hydrateApp()} />
    {:else if $settings.status !== "ready" || $appInfo.status !== "ready"}
      <p class="p-6" role="status">{t("Caricamento...", $language)}</p>
    {:else if !$settings.data.onboardingCompleted}
      <OnboardingView />
    {:else}
      <MainContainer />
    {/if}
  </div>
</div>

{#if $settingsOpen}
  <SettingsDialog onClose={closeSettings} {accent} />
{/if}

<ToastNotice />

{#if $updateState.version && !updateDismissed && ($appInfo.data.platform !== "linux" || !$updateState.nativeNotified)}
  <div class="fixed right-5 bottom-5 z-50 flex max-w-sm items-center gap-3 rounded-xl border border-white/15 bg-zinc-900/95 p-4 text-sm text-white shadow-xl light:border-zinc-900/15 light:bg-white light:text-zinc-900" role="status">
    <span>Legio {$updateState.version}{t(" disponibile.", $language)}{#if $updateState.aur}{t("Aggiorna tramite AUR.", $language)}{/if}</span>
    {#if $appInfo.data.platform !== "linux" && !$updateState.aur}
      <button type="button" class="rounded-lg bg-white px-3 py-2 text-zinc-900 disabled:opacity-50 light:bg-zinc-900 light:text-white" disabled={$updateState.installing} onclick={() => void installAppUpdate()}>
        {$updateState.installing ? t("Installazione...", $language) : t("Aggiorna", $language)}
      </button>
    {/if}
    {#if $appInfo.data.platform !== "linux"}
      <button type="button" class="text-zinc-400 hover:text-white light:hover:text-zinc-900" aria-label={t("Chiudi avviso aggiornamento", $language)} onclick={() => (updateDismissed = true)}>{t("Chiudi", $language)}</button>
    {/if}
  </div>
{/if}
