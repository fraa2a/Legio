<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./lib/components/layout/Sidebar.svelte";
  import MainContainer from "./lib/components/layout/MainContainer.svelte";
  import TitleBar from "./lib/components/layout/TitleBar.svelte";
  import { hydrateApp } from "./lib/stores/bootstrap";
  import { resolvedTheme, settings } from "./lib/stores/settings";

  onMount(() => {
    void hydrateApp();

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
  <div class="flex min-w-0 flex-1 flex-col px-2 pb-2">
    <TitleBar />
    <MainContainer />
  </div>
</div>
