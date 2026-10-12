<script lang="ts">
  import { get } from "svelte/store";
  import { t, language } from "../../i18n";
  import { activeDownloadCount } from "../../stores/downloads";
  import { games } from "../../stores/games";
  import { playtime } from "../../stores/playtime";
  import { settings, updateSettings } from "../../stores/settings";
  import { showToast } from "../../stores/toast";
  import {
    activeSection,
    openSettings,
    sectionLabels,
    sections,
    selectSection,
    selectedGameId,
    type Section,
  } from "../../stores/navigation";
  import { fade } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import Icon from "../ui/Icon.svelte";
  import Logo from "../ui/Logo.svelte";
  import SidebarButton from "../ui/SidebarButton.svelte";
  import SidebarDownloadStatus from "./SidebarDownloadStatus.svelte";
  import SidebarGameItem from "./SidebarGameItem.svelte";
  import { fadeDuration } from "../../utils/motion";
  import { toMessage } from "../../utils/errors";
  import { recentGames } from "../../features/home/home-model";

  const fadeMs = fadeDuration > 0 ? 100 : 0;

  const icons: Record<Section, "home" | "library" | "store" | "downloads"> = {
    home: "home",
    library: "library",
    store: "store",
    downloads: "downloads",
  };

  let expanded = $state(!get(settings).data.sidebarCollapsed);
  let saveQueue: Promise<void> = Promise.resolve();

  function toggleSidebar(): void {
    expanded = !expanded;
    const sidebarCollapsed = !expanded;
    saveQueue = saveQueue.then(async () => {
      try {
        await updateSettings({ sidebarCollapsed });
      } catch (reason) {
        showToast(toMessage(reason), "error");
      }
    });
  }

  const gamesList = $derived(recentGames($games.data, $playtime.data).slice(0, 5).map(({ game }) => game));
</script>

<aside
  class="legio-panel my-1.5 ml-1.5 flex shrink-0 flex-col overflow-hidden rounded-xl bg-zinc-900 light:bg-zinc-50 {expanded
    ? "w-[13.5rem]"
    : "w-16"} transition-[width] duration-300 ease-out"
>
  <nav class="mt-2 flex min-h-0 flex-1 flex-col gap-1 p-2" aria-label={t("Navigazione principale", $language)}>
    <div class="flex w-full items-center text-white light:text-zinc-900">
      <span class="flex h-12 w-full items-center gap-2 overflow-hidden rounded-lg">
        <span class="ml-2 flex size-8 shrink-0 items-center justify-center">
          <Logo />
        </span>
        <span class="overflow-hidden whitespace-nowrap font-mono text-[1.8rem] font-bold tracking-[0.12em]">
          LEGIO
        </span>
      </span>
    </div>
    {#each sections as section (section)}
      <SidebarButton
        label={t(sectionLabels[section], $language)}
        expanded={expanded}
        active={$activeSection === section}
        count={section === "downloads" ? $activeDownloadCount : undefined}
        ariaLabel={section === "downloads" && $activeDownloadCount > 0
          ? t("Download ({0} attivi)", $language, [$activeDownloadCount])
          : undefined}
        onClick={() => selectSection(section)}
      >
        <Icon name={icons[section]} />
      </SidebarButton>
    {/each}
    {#if gamesList.length > 0}
      <div class="flex h-4 items-center px-2">
        <div
          transition:fade={{ duration: fadeMs, easing: cubicOut }}
          class="h-px w-full rounded-full bg-zinc-700/50 light:bg-zinc-400/30"
          aria-hidden="true"
        ></div>
      </div>
      <section
        transition:fade={{ duration: fadeMs, easing: cubicOut }}
        class="mt-2 flex min-h-0 flex-col gap-1.5 overflow-y-auto scrollbar-none"
        aria-label={t("Recenti", $language)}
      >
        <ul class="flex flex-col gap-1.5">
          {#each gamesList as game (game.id)}
            <SidebarGameItem game={game} selected={$selectedGameId === game.id} expanded={expanded} />
          {/each}
        </ul>
      </section>
    {/if}
    <div class="mt-auto flex shrink-0 flex-col gap-1">
      <SidebarDownloadStatus {expanded} />
      <SidebarButton
        label={t("Comprimi", $language)}
        expanded={expanded}
        togglesSidebar
        ariaLabel={expanded ? undefined : t("Espandi sidebar", $language)}
        onClick={toggleSidebar}
      >
        {#if expanded}
          <Icon name="previous" />
        {:else}
          <Icon name="next" />
        {/if}
      </SidebarButton>
      <SidebarButton label={t("Impostazioni", $language)} {expanded} onClick={openSettings}>
        <Icon name="settings" />
      </SidebarButton>
    </div>
  </nav>
</aside>
