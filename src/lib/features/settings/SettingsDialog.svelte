<script lang="ts">
  import { t, language } from "../../i18n";
  import SettingsNavigation from "../../components/ui/SettingsNavigation.svelte";
  import CloseButton from "../../components/ui/CloseButton.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import type { IconName } from "../../components/ui/Icon.svelte";
  import { settingsCategory } from "../../stores/navigation";
  import LibrarySourcesSettings from "./LibrarySourcesSettings.svelte";
  import DownloadSettings from "./DownloadSettings.svelte";
  import CompatibilitySettings from "./CompatibilitySettings.svelte";
  import AdvancedSettings from "./AdvancedSettings.svelte";
  import ThemeSettings from "./ThemeSettings.svelte";
  import GeneralSettings from "./GeneralSettings.svelte";
  import { appInfo } from "../../stores/app-info";

  let { onClose, accent }: { onClose: () => void; accent: string } = $props();

  const categories: { id: string; label: string; icon: IconName }[] = $derived([
    { id: "general", label: t("Generali", $language), icon: "settings" },
    { id: "appearance", label: t("Aspetto", $language), icon: "palette" },
    { id: "downloads", label: "Download", icon: "downloads" },
    { id: "libraries", label: t("Librerie e sorgenti", $language), icon: "folder" },
    ...($appInfo.data.platform === "linux" ? [{ id: "compatibility", label: t("Compatibilità", $language), icon: "wrench" } as const] : []),
    { id: "advanced", label: t("Avanzate", $language), icon: "info" },
  ] as const);

  const requested = $derived($settingsCategory === "sources" ? "libraries" : $settingsCategory);
  const active = $derived(categories.some(category => category.id === requested) ? requested : "general");

</script>

<Dialog open title={t("Impostazioni", $language)} size="large" flush {onClose}>
  {#snippet actions(close)}
    <CloseButton label={t("Chiudi le impostazioni", $language)} onClick={close} />
  {/snippet}

  <div class="flex min-h-0 flex-1">
    <SettingsNavigation {categories} selected={active} label={t("Categorie delle impostazioni", $language)} onSelect={id => settingsCategory.set(id)} />

    <div class="min-w-0 flex-1 overflow-y-auto p-6 lg:p-8">
      {#if active === "general"}
        <GeneralSettings />
      {:else if active === "appearance"}
        <ThemeSettings {accent} />
      {:else if active === "downloads"}
        <DownloadSettings />
      {:else if active === "libraries"}
        <LibrarySourcesSettings />
      {:else if active === "compatibility" && $appInfo.data.platform === "linux"}
        <CompatibilitySettings />
      {:else}
        <AdvancedSettings />
      {/if}
    </div>
  </div>
</Dialog>
