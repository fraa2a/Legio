<script lang="ts">
  import { t, language } from "../../i18n";
  import { activeDownloadCount } from "../../stores/downloads";
  import { games } from "../../stores/games";
  import { playtime } from "../../stores/playtime";
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

  const fadeMs = fadeDuration > 0 ? 100 : 0;

  const icons: Record<Section, "home" | "library" | "store" | "downloads"> = {
    home: "home",
    library: "library",
    store: "store",
    downloads: "downloads",
  };

  let expanded = $state(true);

  const collator = new Intl.Collator(undefined, { sensitivity: "base" });

  const gamesList = $derived.by(() => {
    const lastPlayed = new Map($playtime.data.map((entry) => [entry.gameId, entry.lastPlayedAt]));
    const groups: Record<string, typeof $games.data> = Object.create(null);
    for (const game of $games.data) {
      const key = game.steamAppId === null ? game.id : `steam:${game.steamAppId}`;
      const group = groups[key] ?? [];
      group.push(game);
      groups[key] = group;
    }
    return Object.values(groups)
      .map((group) => group.find((game) => game.steamInstallPath !== null) ?? group[0])
      .map((game) => ({ game, playedAt: lastPlayed.get(game.id) ?? -1 }))
      .filter((entry) => entry.playedAt >= 0)
      .sort((left, right) => right.playedAt - left.playedAt || collator.compare(left.game.name, right.game.name))
      .slice(0, 5)
      .map((entry) => entry.game);
  });
</script>

<aside
  class="my-1.5 ml-1.5 flex shrink-0 flex-col overflow-hidden rounded-xl bg-zinc-900 light:bg-zinc-50 {expanded
    ? "w-[13.5rem]"
    : "w-16"} transition-[width] duration-300 ease-out"
>
  <nav class="mt-2 flex flex-1 flex-col gap-1 p-2" aria-label="Main navigation">
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
        class="mt-2 flex flex-col gap-1.5"
        aria-label={t("Recenti", $language)}
      >
        <ul class="flex flex-col gap-1.5">
          {#each gamesList as game (game.id)}
            <SidebarGameItem game={game} selected={$selectedGameId === game.id} expanded={expanded} />
          {/each}
        </ul>
      </section>
    {/if}
    <div class="mt-auto flex flex-col gap-1">
      <SidebarDownloadStatus {expanded} />
      <SidebarButton
        label={t("Comprimi", $language)}
        expanded={expanded}
        togglesSidebar
        ariaLabel={expanded ? undefined : t("Espandi sidebar", $language)}
        onClick={() => (expanded = !expanded)}
      >
        {#if expanded}
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M15.41,16.58L10.83,12L15.41,7.41L14,6L8,12L14,18L15.41,16.58Z" />
          </svg>
        {:else}
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M8.59,16.58L13.17,12L8.59,7.41L10,6L16,12L10,18L8.59,16.58Z" />
          </svg>
        {/if}
      </SidebarButton>
      <SidebarButton label={t("Impostazioni", $language)} {expanded} onClick={openSettings}>
        <Icon name="settings" />
      </SidebarButton>
    </div>
  </nav>
</aside>
