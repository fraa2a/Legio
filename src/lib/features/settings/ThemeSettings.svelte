<script lang="ts">
  import type { Theme } from "../../services/local-state";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import { changeTheme, settings, settingsError } from "../../stores/settings";

  const themes: { value: Theme; label: string }[] = [
    { value: "system", label: "Sistema" },
    { value: "dark", label: "Scuro" },
    { value: "light", label: "Chiaro" },
  ];

  let selectedTheme: Theme | null = $state(null);

  const currentTheme = $derived(selectedTheme ?? $settings.data.theme);

  async function selectTheme(theme: Theme): Promise<void> {
    selectedTheme = theme;
    await changeTheme(theme);
    selectedTheme = null;
  }
</script>

<section class="flex flex-col gap-4">
  {#if $settingsError}
    <ErrorBanner message={$settingsError} />
  {/if}

  <SettingsGroup
    title="Tema"
    description="Scegli come Legio adatta i colori al sistema o forza uno schema fisso."
  >
    {#each themes as theme (theme.value)}
      <label class="flex cursor-pointer items-center justify-between gap-6">
        <span class="text-sm text-zinc-100 light:text-zinc-900">{theme.label}</span>
        <input
          type="radio"
          name="theme"
          value={theme.value}
          checked={currentTheme === theme.value}
          onchange={() => void selectTheme(theme.value)}
          class="size-4 accent-white"
        />
      </label>
    {/each}
  </SettingsGroup>
</section>