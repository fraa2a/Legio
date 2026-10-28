<script lang="ts">
  import { t, language } from "../../i18n";
  import SettingsNavigation from "../../components/ui/SettingsNavigation.svelte";
  import CloseButton from "../../components/ui/CloseButton.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import { settingsCategory } from "../../stores/navigation";
  import LibrarySourcesSettings from "./LibrarySourcesSettings.svelte";
  import DownloadSettings from "./DownloadSettings.svelte";
  import CompatibilitySettings from "./CompatibilitySettings.svelte";
  import AdvancedSettings from "./AdvancedSettings.svelte";
  import ThemeSettings from "./ThemeSettings.svelte";
  import GeneralSettings from "./GeneralSettings.svelte";
  import { settingsCategories } from "./settings-categories";
  import { appInfo } from "../../stores/app-info";
  import logo from "../../../assets/logo.png";

  let { onClose, accent }: { onClose: () => void; accent: string } = $props();

  const categories = $derived(settingsCategories($appInfo.data.platform).map((category) => ({ ...category, label: t(category.label, $language) })));

  const requested = $derived($settingsCategory === "sources" ? "libraries" : $settingsCategory);
  const active = $derived(categories.some(category => category.id === requested) ? requested : "general");

</script>

<Dialog open title={t("Impostazioni", $language)} size="large" flush {onClose}>
  {#snippet actions(close)}
    <CloseButton label={t("Chiudi le impostazioni", $language)} onClick={close} />
  {/snippet}

  <div class="flex min-h-0 flex-1">
    <SettingsNavigation {categories} selected={active} label={t("Categorie delle impostazioni", $language)} onSelect={id => settingsCategory.set(id)}>
      {#snippet footer()}
        <div class="flex items-center gap-2 px-3 py-2">
          <img src={logo} alt="" class="size-8 shrink-0 rounded-lg" />
          <div class="flex min-w-0 flex-col leading-tight">
            <span class="truncate text-sm font-semibold text-zinc-100 light:text-zinc-900">Legio</span>
            {#if $appInfo.data.version !== ""}<span class="truncate text-xs text-zinc-400 light:text-zinc-600">v.{$appInfo.data.version}</span>{/if}
          </div>
        </div>
      {/snippet}
    </SettingsNavigation>

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
