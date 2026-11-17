<script lang="ts">
  import { t, language } from "../../i18n";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import ToggleSetting from "../../components/ui/ToggleSetting.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import Button from "../../components/ui/Button.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import SteamExportDialog from "../library/SteamExportDialog.svelte";
  import { isSteamExportable } from "../library/steam-export";
  import { games } from "../../stores/games";
  import { configureSteamScanInterval } from "../../stores/bootstrap";
  import { importSteamLibrary } from "../../stores/steam-library";
  import { defaultSettings, settings, settingsError, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  let exportOpen = $state(false);
  const exportable = $derived($games.data.filter(isSteamExportable));
  const intervals = [5, 10, 15, 30, 60, 120];
  const options = $derived(intervals.map((minutes) => ({ value: String(minutes), label: t("{0} minuti", $language, [minutes]) })));
  async function selectInterval(value: string): Promise<void> {
    const minutes = Number(value);
    if (!intervals.includes(minutes) || minutes === $settings.data.steamLibraryPollMinutes) return;
    settingsError.set(null);
    try {
      const updated = await updateSettings({ steamLibraryPollMinutes: minutes });
      configureSteamScanInterval(updated.steamLibraryPollMinutes);
    } catch (error) {
      settingsError.set(toMessage(error));
    }
  }


  async function updateSync(enabled: boolean): Promise<void> {
    settingsError.set(null);
    try {
      const updated = await updateSettings({ syncSteamGames: enabled });
      configureSteamScanInterval(updated.steamLibraryPollMinutes);
      if (updated.syncSteamGames) void importSteamLibrary();
    } catch (error) {
      settingsError.set(toMessage(error));
    }
  }
</script>

{#if $settingsError}<ErrorBanner message={t("Impossibile salvare le impostazioni.", $language)} detail={$settingsError} />{/if}
  <SettingsGroup
    icon="timer"
    title={t("Controllo Steam", $language)}
  >
    <ToggleSetting label={t("Sync Steam games", $language)} checked={$settings.data.syncSteamGames}
      defaultValue={defaultSettings.syncSteamGames} onChange={(checked) => void updateSync(checked)} />
    <p class="text-xs text-legio-fg-muted">{t("Quando è disattivato, Legio non controlla i giochi Steam all'avvio o periodicamente. I giochi già importati restano nella libreria.", $language)}</p>
    <div class="flex max-w-64 items-end gap-2"><div class="min-w-0 flex-1">
      <SelectField
        id="steam-library-poll-interval"
        label={t("Intervallo di controllo", $language)}
        value={String($settings.data.steamLibraryPollMinutes)}
        {options}
        borderless
        disabled={!$settings.data.syncSteamGames || $settings.status === "loading" || $settings.status === "idle"}
        onChange={(value) => void selectInterval(value)}
      />
    </div><ResetSetting label={t("Ripristina intervallo di controllo", $language)} disabled={$settings.data.steamLibraryPollMinutes === defaultSettings.steamLibraryPollMinutes} onClick={() => void selectInterval(String(defaultSettings.steamLibraryPollMinutes))} /></div>
  </SettingsGroup>
  <SettingsGroup icon="gamepad" title={t("Collegamenti Steam", $language)}>
    <SettingsRow label={t("Esporta in Steam", $language)} description={t("Aggiunge i giochi di Legio a Steam come giochi non Steam, per Steam Deck e Big Picture. Steam deve essere chiuso.", $language)}>
      <Button label={t("Esporta in Steam", $language)} variant="secondary" disabled={exportable.length === 0} onClick={() => { exportOpen = true; }} />
    </SettingsRow>
  </SettingsGroup>
{#if exportOpen}<SteamExportDialog games={exportable} onClose={() => { exportOpen = false; }} />{/if}
