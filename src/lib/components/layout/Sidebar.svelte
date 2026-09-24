<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import Logo from "../ui/Logo.svelte";
  import SidebarButton from "../ui/SidebarButton.svelte";

  type Section = "home" | "library" | "store" | "settings";

  let activeSection = $state<Section>("home");
  let expanded = $state(true);
</script>

<aside
  class="flex h-full shrink-0 flex-col overflow-hidden rounded-2xl bg-zinc-900 {expanded
    ? "w-[13.5rem]"
    : "w-16"} transition-[width] duration-300 ease-out"
>
  <nav class="mt-2 flex flex-1 flex-col gap-1 p-2" aria-label="Main navigation">
    <div class="flex w-full items-center text-white">
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
    <SidebarButton label="Home" expanded={expanded} active={activeSection === "home"} onClick={() => (activeSection = "home")}>
      <Icon name="home" />
    </SidebarButton>
    <SidebarButton label="Libreria" expanded={expanded} active={activeSection === "library"} onClick={() => (activeSection = "library")}>
      <Icon name="library" />
    </SidebarButton>
    <SidebarButton label="Store" expanded={expanded} active={activeSection === "store"} onClick={() => (activeSection = "store")}>
      <Icon name="store" />
    </SidebarButton>
    <div class="mt-auto flex flex-col gap-1">
      <SidebarButton
        label="Comprimi"
        expanded={expanded}
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
        label="Impostazioni"
        expanded={expanded}
        active={activeSection === "settings"}
        onClick={() => (activeSection = "settings")}
      >
        <Icon name="settings" />
      </SidebarButton>
    </div>
  </nav>
</aside>