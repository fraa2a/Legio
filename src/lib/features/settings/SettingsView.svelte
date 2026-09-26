<script lang="ts">
  import type { Theme } from "../../services/local-state";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { checkConnectivity, connectivityError, networkSummary } from "../../stores/network";
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

<section class="mb-8">
  <h2 class="mb-3 text-lg font-medium text-zinc-100 light:text-zinc-800">Tema</h2>

  {#if $settingsError}
    <div class="mb-3">
      <ErrorBanner message={$settingsError} />
    </div>
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

<section class="mb-8">
  <h2 class="mb-3 text-lg font-medium text-zinc-100 light:text-zinc-800">Rete</h2>

  <div class="flex flex-wrap items-center gap-3 text-sm text-zinc-300 light:text-zinc-700">
    <span>Stato: {$networkSummary.status}</span>
    <span class="text-zinc-400 light:text-zinc-600">Steam: {$networkSummary.steam}</span>
  </div>

  {#if $networkSummary.detail}
    <p class="mt-1 text-xs text-zinc-500">Dettaglio: {$networkSummary.detail}</p>
  {/if}

  {#if $connectivityError}
    <div class="mt-3">
      <ErrorBanner message={$connectivityError} onRetry={() => void checkConnectivity()} />
    </div>
  {/if}

  <div class="mt-3">
    <Button label="Verifica connettività" variant="secondary" onClick={() => void checkConnectivity()} />
  </div>
</section>
