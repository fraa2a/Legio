<script lang="ts">
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import { saveSettings } from "../../services/local-state";
  import { configureSteamScanInterval } from "../../stores/bootstrap";
  import { settings, settingsError } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";

  const intervals = [5, 10, 15, 30, 60, 120];
  const options = intervals.map((minutes) => ({ value: String(minutes), label: `${minutes} minuti` }));
  let saving = $state(false);
  let saved = $state(false);

  async function selectInterval(value: string): Promise<void> {
    const minutes = Number(value);
    if (!intervals.includes(minutes) || minutes === $settings.data.steamLibraryPollMinutes) return;
    saving = true;
    saved = false;
    settingsError.set(null);
    try {
      const updated = await saveSettings({ ...$settings.data, steamLibraryPollMinutes: minutes });
      settings.set(updated);
      configureSteamScanInterval(updated.steamLibraryPollMinutes);
      saved = true;
    } catch (error) {
      settingsError.set(toMessage(error));
    } finally {
      saving = false;
    }
  }
</script>

<section class="flex flex-col gap-4">
  <div>
    <h3 class="text-xl font-semibold text-zinc-100 light:text-zinc-900">Generali</h3>
    <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">Configura il controllo automatico della libreria Steam.</p>
  </div>
  {#if $settingsError}<ErrorBanner message={$settingsError} />{/if}
  <div class="max-w-xl">
    <SelectField
      id="steam-library-poll-interval"
      label="Intervallo di controllo Steam"
      value={String($settings.data.steamLibraryPollMinutes)}
      {options}
      disabled={saving || $settings.status === "loading" || $settings.status === "idle"}
      onChange={(value) => void selectInterval(value)}
    />
    <p class="mt-2 text-xs text-zinc-500">Legio controlla Steam all'avvio e ripete il controllo all'intervallo scelto.</p>
  </div>
  {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Intervallo salvato.</p>{/if}
</section>
