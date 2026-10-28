<script lang="ts">
  import { t, language } from "../../i18n";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import ToggleSetting from "../../components/ui/ToggleSetting.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import { configureSteamScanInterval } from "../../stores/bootstrap";
  import { importSteamLibrary } from "../../stores/steam-library";
  import { defaultSettings, settings, settingsError, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  let saving = $state(false);
  let saved = $state(false);
  const intervals = [5, 10, 15, 30, 60, 120];
  const options = $derived(intervals.map((minutes) => ({ value: String(minutes), label: t("{0} minuti", $language, [minutes]) })));
  async function selectInterval(value: string): Promise<void> {
    const minutes = Number(value);
    if (!intervals.includes(minutes) || minutes === $settings.data.steamLibraryPollMinutes) return;
    saving = true;
    saved = false;
    settingsError.set(null);
    try {
      const updated = await updateSettings({ steamLibraryPollMinutes: minutes });
      configureSteamScanInterval(updated.steamLibraryPollMinutes);
      saved = true;
    } catch (error) {
      settingsError.set(toMessage(error));
    } finally {
      saving = false;
    }
  }


  async function updateSync(enabled: boolean): Promise<void> {
    saving = true;
    saved = false;
    settingsError.set(null);
    try {
      const updated = await updateSettings({ syncSteamGames: enabled });
      configureSteamScanInterval(updated.steamLibraryPollMinutes);
      if (updated.syncSteamGames) void importSteamLibrary();
      saved = true;
    } catch (error) {
      settingsError.set(toMessage(error));
    } finally {
      saving = false;
    }
  }
</script>

{#if $settingsError}<ErrorBanner message={$settingsError} />{/if}
  <SettingsGroup
    icon="store"
    title={t("Controllo Steam", $language)}
  >
    <ToggleSetting label={t("Sync Steam games", $language)} checked={$settings.data.syncSteamGames}
      defaultValue={defaultSettings.syncSteamGames} disabled={saving} onChange={(checked) => void updateSync(checked)} />
    <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Quando è disattivato, Legio non controlla i giochi Steam all'avvio o periodicamente. I giochi già importati restano nella libreria.", $language)}</p>
    <div class="flex max-w-64 items-end gap-2"><div class="min-w-0 flex-1">
      <SelectField
        id="steam-library-poll-interval"
        label={t("Intervallo di controllo", $language)}
        value={String($settings.data.steamLibraryPollMinutes)}
        {options}
        borderless
        disabled={saving || !$settings.data.syncSteamGames || $settings.status === "loading" || $settings.status === "idle"}
        onChange={(value) => void selectInterval(value)}
      />
    </div><ResetSetting label={t("Ripristina intervallo di controllo", $language)} disabled={saving || $settings.data.steamLibraryPollMinutes === defaultSettings.steamLibraryPollMinutes} onClick={() => void selectInterval(String(defaultSettings.steamLibraryPollMinutes))} /></div>
  </SettingsGroup>
{#if saved}<Callout tone="success" variant="inline" message={t("Impostazioni salvate.", $language)} />{/if}
