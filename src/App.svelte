<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./lib/components/layout/Sidebar.svelte";
  import MainContainer from "./lib/components/layout/MainContainer.svelte";
  import SplashScreen from "./lib/components/layout/SplashScreen.svelte";
  import TitleBar from "./lib/components/layout/TitleBar.svelte";
  import SettingsDialog from "./lib/features/settings/SettingsDialog.svelte";
  import { hydrateApp } from "./lib/stores/bootstrap";
  import { closeSettings, settingsOpen } from "./lib/stores/navigation";
  import { resolvedTheme, settings } from "./lib/stores/settings";

  let ready = $state(false);

  onMount(() => {
    void hydrateApp()
      .catch((reason) => console.error(reason))
      .finally(() => (ready = true));

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

  function blockTabFocus(event: KeyboardEvent) {
    if (event.key === "Tab") event.preventDefault();
  }
</script>

<svelte:head>
  <title>Legio</title>
</svelte:head>

<svelte:window oncontextmenu={blockContextMenu} onkeydown={blockTabFocus} />

{#if ready}
  <div class="flex h-dvh select-none bg-black light:bg-zinc-200">
    <Sidebar />
    <div class="flex min-w-0 flex-1 flex-col pl-2.5 pr-1.5 pb-1.5">
      <TitleBar />
      <MainContainer />
    </div>
  </div>
{:else}
  <SplashScreen />
{/if}

{#if $settingsOpen}
  <SettingsDialog onClose={closeSettings} />
{/if}
