<script lang="ts">
  import type { Theme } from "../../services/local-state";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
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
  <div>
    <h3 class="text-xl font-semibold text-zinc-100 light:text-zinc-900">Tema</h3>
    <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">
      Scegli come Legio adatta i colori al sistema o forza uno schema fisso.
    </p>
  </div>

  {#if $settingsError}
    <ErrorBanner message={$settingsError} />
  {/if}

  <fieldset class="flex flex-wrap gap-2">
    <legend class="sr-only">Tema dell'interfaccia</legend>
    {#each themes as theme (theme.value)}
      <label
        class="flex cursor-pointer items-center gap-2 rounded-lg bg-white/5 px-4 py-2 text-sm text-zinc-200 transition-colors duration-200 hover:bg-white/10 has-checked:bg-white/20 light:bg-zinc-100 light:text-zinc-800"
      >
        <input
          type="radio"
          name="theme"
          value={theme.value}
          checked={currentTheme === theme.value}
          onchange={() => void selectTheme(theme.value)}
          class="size-4 accent-white"
        />
        {theme.label}
      </label>
    {/each}
  </fieldset>
</section>
