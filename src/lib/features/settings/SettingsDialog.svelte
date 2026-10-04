<script lang="ts">
  import { t, language } from "../../i18n";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Icon, { type IconName } from "../../components/ui/Icon.svelte";
  import { fade } from "svelte/transition";
  import { fadeDuration } from "../../utils/motion";
  import DownloadSettings from "./DownloadSettings.svelte";
  import CompatibilitySettings from "./CompatibilitySettings.svelte";
  import AdvancedSettings from "./AdvancedSettings.svelte";
  import ThemeSettings from "./ThemeSettings.svelte";
  import GeneralSettings from "./GeneralSettings.svelte";
  import { appInfo } from "../../stores/app-info";

  let { onClose }: { onClose: () => void } = $props();

  const categories: { id: string; label: string; icon: IconName }[] = $derived([
    { id: "general", label: t("Generali", $language), icon: "settings" },
    { id: "appearance", label: t("Aspetto", $language), icon: "palette" },
    { id: "downloads", label: "Download", icon: "downloads" },
    ...($appInfo.data.platform === "linux" ? [{ id: "compatibility", label: t("Compatibilità", $language), icon: "wrench" } as const] : []),
    { id: "advanced", label: t("Avanzate", $language), icon: "info" },
  ] as const);

  type CategoryId = (typeof categories)[number]["id"];

  let active = $state<CategoryId>("general");

  function categoryClass(isActive: boolean): string {
    return isActive
      ? "bg-white/10 font-semibold text-white light:bg-zinc-900/10 light:text-zinc-900"
      : "font-medium text-zinc-400 hover:bg-white/5 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/5 light:hover:text-zinc-900";
  }
</script>

<Dialog open title={t("Impostazioni", $language)} size="large" flush {onClose}>
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
      class="flex w-60 shrink-0 flex-col gap-1 overflow-y-auto border-r border-white/15 p-3 light:border-zinc-900/15"
      aria-label={t("Categorie delle impostazioni", $language)}
    >
      {#each categories as category (category.id)}
        <button
          type="button"
          aria-current={active === category.id ? "true" : undefined}
          class="flex items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm transition-colors duration-200 {categoryClass(
            active === category.id,
          )}"
          onclick={() => (active = category.id)}
        >
          <Icon name={category.icon} size="size-4 shrink-0" />
          {category.label}
        </button>
      {/each}
    </nav>

    <div class="min-w-0 flex-1 overflow-y-auto p-6 lg:p-8">
      {#key active}<div in:fade={{ duration: fadeDuration }}>
      {#if active === "general"}
        <GeneralSettings />
      {:else if active === "appearance"}
        <ThemeSettings />
      {:else if active === "downloads"}
        <DownloadSettings />
      {:else if active === "compatibility" && $appInfo.data.platform === "linux"}
        <CompatibilitySettings />
      {:else}
        <AdvancedSettings />
      {/if}
      </div>{/key}
    </div>
  </div>
</Dialog>
