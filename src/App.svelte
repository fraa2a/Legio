<script lang="ts">
  import { t, language } from "./lib/i18n";
  import { activePalette, applyAppearance, resolvedAccent } from "./lib/services/appearance";
  import { appearancePreview } from "./lib/stores/appearance";
  import { gameArtColor } from "./lib/stores/game-accent";
  import { adjustAccent } from "./lib/services/game-accent";
  import { appInfo } from "./lib/stores/app-info";
  import AppBackground from "./lib/components/layout/AppBackground.svelte";
  import { startArtworkSelectionTracking } from "./lib/services/artwork-work";
  import { onMount } from "svelte";
  import { derived, get } from "svelte/store";
  import Sidebar from "./lib/components/layout/Sidebar.svelte";
  import OnboardingView from "./lib/features/onboarding/OnboardingView.svelte";
  import DatabaseRecoveryView from "./lib/features/recovery/DatabaseRecoveryView.svelte";
  import ErrorBanner from "./lib/components/ui/ErrorBanner.svelte";
  import MainContainer from "./lib/components/layout/MainContainer.svelte";
  import TitleBar from "./lib/components/layout/TitleBar.svelte";
  import SettingsDialog from "./lib/features/settings/SettingsDialog.svelte";
  import ToastNotice from "./lib/components/ui/ToastNotice.svelte";
  import DiskUsageDialog from "./lib/features/settings/DiskUsageDialog.svelte";
  import AccountSwitchDialog from "./lib/features/library/AccountSwitchDialog.svelte";
  import ModLinkDialog from "./lib/features/modding/ModLinkDialog.svelte";
  import SourceLinkDialog from "./lib/features/settings/SourceLinkDialog.svelte";
  import ThemeLinkDialog from "./lib/features/settings/ThemeLinkDialog.svelte";
  import { checkForAppUpdateWhenEnabled, hydrateApp } from "./lib/stores/bootstrap";
  import { diskUsageDialogOpen } from "./lib/stores/disk-usage";
  import { shortcutsOpen } from "./lib/stores/shortcuts";
  import { startNotificationEvents } from "./lib/stores/notification-events";
  import { handleShortcutKey } from "./lib/features/shortcuts/shortcut-actions";
  import ShortcutsDialog from "./lib/features/shortcuts/ShortcutsDialog.svelte";
  import { closeSettings, settingsOpen } from "./lib/stores/navigation";
  import { settings } from "./lib/stores/settings";
  import { databaseFailure } from "./lib/stores/database-recovery";
  import { updateState } from "./lib/services/app-updater";
  import { startPeriodicChecks } from "./lib/services/update-schedule";
  import ReleaseNotesDialog from "./lib/features/updates/ReleaseNotesDialog.svelte";
  import AppUpdateDialog from "./lib/features/updates/AppUpdateDialog.svelte";
  import { windowActive, windowVisible } from "./lib/stores/window-activity";
  import { hasRunningGame } from "./lib/stores/launch";
  import { observeMediaPlayback } from "./lib/services/media-playback";
  import { startArtworkDisplayTracking } from "./lib/services/artwork-display";
  import { applicationErrorStack, describeApplicationError, logApplicationEvent } from "./lib/services/application-log";
  import { allowsNativeContextMenu } from "./lib/utils/context-menu";

  const mediaActive = derived([windowVisible, hasRunningGame], ([$visible, $running]) => $visible && !$running);
  const backgroundActive = derived([windowActive, hasRunningGame], ([$active, $running]) => $active && !$running);
  const appearance = $derived($appearancePreview?.appearance ?? $settings.data.appearance);
  const theme = $derived($appearancePreview?.theme ?? $settings.data.theme);
  let prefersDark = $state(window.matchMedia("(prefers-color-scheme: dark)").matches);
  const palette = $derived(activePalette(theme, prefersDark));
  const gameAccent = $derived(appearance.gameAccent && $gameArtColor !== null ? adjustAccent($gameArtColor.color, palette.palette, palette.scheme) : null);
  const accent = $derived(gameAccent ?? resolvedAccent(palette, appearance));

  onMount(() => {
    const stopArtworkSelection = startArtworkSelectionTracking();
    const stopArtworkDisplays = startArtworkDisplayTracking();
    const stopMedia = observeMediaPlayback(mediaActive.subscribe);
    const stopNotificationEvents = startNotificationEvents();
    void hydrateApp().then(() => {
      logApplicationEvent("info", "renderer_ready", "Application resources loaded");
    }).catch(reason => {
      logApplicationEvent("error", "bootstrap_error", describeApplicationError(reason), applicationErrorStack(reason));
      console.error("Application resource initialization failed", reason);
    });
    // Both the first and the periodic checks honor the automatic update check switch.
    const stopUpdateChecks = import.meta.env.PROD
      ? startPeriodicChecks(checkForAppUpdateWhenEnabled, () => get(hasRunningGame) || get(updateState).installing)
      : () => {};
    if (import.meta.env.PROD) void checkForAppUpdateWhenEnabled();

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const updateSystemTheme = () => {
      prefersDark = media.matches;
    };

    media.addEventListener("change", updateSystemTheme);

    return () => {
      stopArtworkSelection();
      stopArtworkDisplays();
      stopMedia();
      stopUpdateChecks();
      stopNotificationEvents();
      media.removeEventListener("change", updateSystemTheme);
    };
  });

  $effect(() => {
    applyAppearance(document.documentElement, theme, appearance, prefersDark,
      $appInfo.data.platform === "linux" && $appInfo.data.desktopEnvironment === "hyprland", gameAccent);
  });

  function blockContextMenu(event: MouseEvent) {
    if (!allowsNativeContextMenu(event.target)) event.preventDefault();
  }
</script>

<svelte:head>
  <title>Legio</title>
</svelte:head>

<svelte:window
  oncontextmenu={blockContextMenu}
  ondragstart={(event) => { if (event.target instanceof HTMLImageElement) event.preventDefault(); }}
  onpointerdown={() => { document.documentElement.dataset.inputMethod = "pointer"; }}
  onkeydown={(event) => {
    if (event.key === "Tab" || event.key.startsWith("Arrow")) document.documentElement.dataset.inputMethod = "keyboard";
    if ($settings.status === "ready" && $settings.data.onboardingCompleted) handleShortcutKey(event);
  }}
/>

<AppBackground id={appearance.background} animation={appearance.animatedBackground} animationOpacity={appearance.animatedOpacity} {accent} dither={appearance.dither} {palette} season={appearance.season} starDensity={appearance.starDensity} grain={appearance.grain} active={$backgroundActive} />
<div class="legio-shell relative flex h-full w-full min-h-0 min-w-0 select-none">
  {#if $settings.status === "ready" && $settings.data.onboardingCompleted}<Sidebar />{/if}
  <div class="flex min-h-0 min-w-0 flex-1 flex-col pl-2.5 pr-2.5 pb-1.5">
    <TitleBar />
    {#if $databaseFailure.data !== null}
      <DatabaseRecoveryView failure={$databaseFailure.data} />
    {:else if $settings.status === "error" || $appInfo.status === "error"}
      <ErrorBanner message={t("Impossibile caricare la configurazione.", $language)} detail={$settings.error ?? $appInfo.error} onRetry={() => void hydrateApp()} />
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

{#if $diskUsageDialogOpen}
  <DiskUsageDialog onClose={() => diskUsageDialogOpen.set(false)} />
{/if}
{#if $shortcutsOpen}
  <ShortcutsDialog />
{/if}

<AccountSwitchDialog />
{#if $settings.status === "ready" && $settings.data.onboardingCompleted}
  <ModLinkDialog />
{/if}
<SourceLinkDialog />
<ReleaseNotesDialog />
<AppUpdateDialog />
<ThemeLinkDialog />
<ToastNotice />
