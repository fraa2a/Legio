<script lang="ts">
  import { activeDownloadCount } from "../../stores/downloads";
  import { activeSection, sectionLabels, sections, selectSection, type Section } from "../../stores/navigation";
  import Icon from "../ui/Icon.svelte";
  import Logo from "../ui/Logo.svelte";
  import SidebarButton from "../ui/SidebarButton.svelte";

  const icons: Record<Section, "home" | "library" | "store" | "downloads" | "settings"> = {
    home: "home",
    library: "library",
    store: "store",
    downloads: "downloads",
    settings: "settings",
  };

  const navSections = sections.filter((section) => section !== "settings");

  let expanded = $state(true);
</script>

<aside
  class="my-2 ml-2 flex shrink-0 flex-col overflow-hidden rounded-2xl bg-zinc-900 light:bg-zinc-50 {expanded
    ? "w-[13.5rem]"
    : "w-16"} transition-[width] duration-300 ease-out"
>
  <nav class="mt-2 flex flex-1 flex-col gap-1 p-2" aria-label="Main navigation">
    <div class="flex w-full items-center text-white light:text-zinc-900">
      <span
        class="flex h-12 items-center gap-2 overflow-hidden rounded-lg transition-[width] duration-300 ease-out {expanded
          ? "w-50"
          : "w-12"}"
      >
        <span class="ml-2 flex size-8 shrink-0 items-center justify-center">
          <Logo />
        </span>
        <span class="overflow-hidden whitespace-nowrap text-4xl font-semibold tracking-[0.2em]">
          LEGIO
        </span>
      </span>
    </div>
    {#each navSections as section (section)}
      <SidebarButton
        label={sectionLabels[section]}
        expanded={expanded}
        active={$activeSection === section}
        count={section === "downloads" ? $activeDownloadCount : undefined}
        ariaLabel={section === "downloads" && $activeDownloadCount > 0
          ? `${sectionLabels.downloads} (${$activeDownloadCount} attivi)`
          : undefined}
        onClick={() => selectSection(section)}
      >
        <Icon name={icons[section]} />
      </SidebarButton>
    {/each}
    <div class="mt-auto flex flex-col gap-1">
      <SidebarButton
        label="Comprimi"
        expanded={expanded}
        togglesSidebar
        ariaLabel={expanded ? undefined : "Espandi sidebar"}
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
      <SidebarButton
        label={sectionLabels.settings}
        expanded={expanded}
        active={$activeSection === "settings"}
        onClick={() => selectSection("settings")}
      >
        <Icon name="settings" />
      </SidebarButton>
    </div>
  </nav>
</aside>
