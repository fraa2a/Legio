<script lang="ts">
  import { t, language } from "../../i18n";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import DownloadSettings from "./DownloadSettings.svelte";
  import CompatibilitySettings from "./CompatibilitySettings.svelte";
  import NetworkSettings from "./NetworkSettings.svelte";
  import ThemeSettings from "./ThemeSettings.svelte";
  import GeneralSettings from "./GeneralSettings.svelte";

  let { onClose }: { onClose: () => void } = $props();

  const categories = $derived([
    { id: "general", label: t("Generali", $language) },
    { id: "appearance", label: t("Aspetto", $language) },
    { id: "network", label: t("Rete", $language) },
    { id: "downloads", label: "Download" },
    { id: "compatibility", label: t("Compatibilità", $language) },
  ] as const);

  type Category = (typeof categories)[number]["id"];

  let active = $state<Category>("general");

  function categoryClass(isActive: boolean): string {
    return isActive
      ? "bg-white/10 text-white light:bg-zinc-900/10 light:text-zinc-900"
      : "text-zinc-400 hover:bg-white/5 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/5 light:hover:text-zinc-900";
  }
</script>

<Dialog open title={t("Impostazioni", $language)} size="wide" flush {onClose}>
  {#snippet actions()}
    <button
      type="button"
      aria-label={t("Chiudi le impostazioni", $language)}
      class="rounded-lg p-1.5 text-zinc-400 transition-colors duration-200 hover:bg-white/10 hover:text-zinc-100 light:text-zinc-500 light:hover:bg-zinc-900/10 light:hover:text-zinc-900"
      onclick={onClose}
    >
      <Icon name="close" size="h-5 w-5" />
    </button>
  {/snippet}

  <div class="flex min-h-0 flex-1">
    <nav
      class="flex w-52 shrink-0 flex-col gap-1 overflow-y-auto border-r border-white/15 p-2 light:border-zinc-900/15"
      aria-label={t("Categorie delle impostazioni", $language)}
    >
      {#each categories as category (category.id)}
        <button
          type="button"
          aria-current={active === category.id ? "true" : undefined}
          class="rounded-lg px-3 py-2 text-left text-sm font-medium transition-colors duration-200 {categoryClass(
            active === category.id,
          )}"
          onclick={() => (active = category.id)}
        >
          {category.label}
        </button>
      {/each}
    </nav>

    <div class="min-w-0 flex-1 overflow-y-auto p-6">
      {#if active === "general"}
        <GeneralSettings />
      {:else if active === "appearance"}
        <ThemeSettings />
      {:else if active === "network"}
        <NetworkSettings />
      {:else if active === "downloads"}
        <DownloadSettings />
      {:else}
        <CompatibilitySettings />
      {/if}
    </div>
  </div>
</Dialog>
