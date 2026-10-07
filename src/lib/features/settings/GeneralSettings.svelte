<script lang="ts">
  import DiscordPresenceSettings from "./DiscordPresenceSettings.svelte";
  import { t, language } from "../../i18n";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";
  import Button from "../../components/ui/Button.svelte";
  import { saveSettings, type Settings } from "../../services/local-state";
  import { configureSteamScanInterval } from "../../stores/bootstrap";
  import { closeSettings } from "../../stores/navigation";
  import { defaultSettings, settings, settingsError } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import { checkForAppUpdate, installAppUpdate, updateState } from "../../services/app-updater";

  const intervals = [5, 10, 15, 30, 60, 120];
  const options = $derived(intervals.map((minutes) => ({ value: String(minutes), label: t("{0} minuti", $language, [minutes]) })));
  const behaviors: { key: keyof Settings; label: string }[] = $derived([
    { key: "closeToTray", label: t("Riduci nell'area di notifica alla chiusura", $language) },
    { key: "hideOnGameStart", label: t("Nascondi Legio nell'area di notifica quando avvii un gioco", $language) },
    { key: "launchOnSystemStart", label: t("Avvia Legio all'accesso al sistema", $language) },
    { key: "launchMinimized", label: t("Avvia Legio ridotto nell'area di notifica", $language) },
    { key: "deferExtractionWhilePlaying", label: t("Posticipa l’estrazione automatica dei download mentre giochi", $language) },
    { key: "launchInLibrary", label: t("Apri Legio sulla Libreria", $language) },
  ]);
  let saving = $state(false);
  let saved = $state(false);

  async function selectLanguage(value: string): Promise<void> {
    if (value !== "system" && value !== "it" && value !== "en") return;
    await update({ language: value });
  }

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

  async function update(changes: Partial<Settings>): Promise<boolean> {
    saving = true;
    saved = false;
    settingsError.set(null);
    try {
      settings.set(await saveSettings({ ...$settings.data, ...changes }));
      if (changes.onboardingCompleted === false) closeSettings();
      saved = true;
      return true;
    } catch (error) {
      settingsError.set(toMessage(error));
      return false;
    } finally {
      saving = false;
    }
  }


</script>

<section class="flex flex-col gap-4">
  {#if $settingsError}
    <ErrorBanner message={$settingsError} />
  {/if}

  <SettingsGroup icon="translate" title={t("Lingua", $language)}>
    <div class="flex items-end gap-2"><div class="min-w-0 flex-1"><SelectField id="ui-language" label={t("Lingua dell'interfaccia", $language)} value={$settings.data.language}
      options={[{ value: "system", label: t("Sistema", $language) }, { value: "it", label: "Italiano" }, { value: "en", label: "English" }]}
      borderless disabled={saving} onChange={(value) => void selectLanguage(value)} /></div>
      <ResetSetting label={t("Ripristina lingua", $language)} disabled={saving || $settings.data.language === defaultSettings.language} onClick={() => void update({ language: defaultSettings.language })} />
    </div>
  </SettingsGroup>
  <SettingsGroup icon="settings" title={t("Comportamento", $language)}>
    {#each behaviors as option (option.key)}
      <div class="flex items-center gap-2"><div class="min-w-0 flex-1">
      <Toggle
        label={option.label}
        checked={Boolean($settings.data[option.key])}
        disabled={saving || (option.key === "launchMinimized" && !$settings.data.launchOnSystemStart)}
        onChange={(checked) => void update({ [option.key]: checked,
          ...(option.key === "launchOnSystemStart" && !checked ? { launchMinimized: false } : {}) })}
      />
      </div><ResetSetting label={t("Ripristina {0}", $language, [option.label])} disabled={saving || $settings.data[option.key] === defaultSettings[option.key]} onClick={() => void update({ [option.key]: defaultSettings[option.key], ...(option.key === "launchOnSystemStart" && !defaultSettings.launchOnSystemStart ? { launchMinimized: false } : {}) })} /></div>
    {/each}
  </SettingsGroup>

  <DiscordPresenceSettings />

  <SettingsGroup
    icon="store"
    title={t("Controllo Steam", $language)}
  >
    <div class="flex max-w-64 items-end gap-2"><div class="min-w-0 flex-1">
      <SelectField
        id="steam-library-poll-interval"
        label={t("Intervallo di controllo", $language)}
        value={String($settings.data.steamLibraryPollMinutes)}
        {options}
        borderless
        disabled={saving || $settings.status === "loading" || $settings.status === "idle"}
        onChange={(value) => void selectInterval(value)}
      />
    </div><ResetSetting label={t("Ripristina intervallo di controllo", $language)} disabled={saving || $settings.data.steamLibraryPollMinutes === defaultSettings.steamLibraryPollMinutes} onClick={() => void selectInterval(String(defaultSettings.steamLibraryPollMinutes))} /></div>
  </SettingsGroup>

  <SettingsGroup icon="settings" title={t("Configurazione iniziale", $language)}>
    <Button label={t("Ripeti configurazione iniziale", $language)} variant="secondary" disabled={saving} onClick={() => void update({ onboardingCompleted: false })} />
  </SettingsGroup>

  <SettingsGroup icon="reload" title={t("Aggiornamenti", $language)}>
    <div class="flex flex-wrap items-center gap-3">
      <Button label={$updateState.checking ? t("Controllo...", $language) : t("Controlla aggiornamenti", $language)} variant="secondary" disabled={$updateState.checking || $updateState.installing} onClick={() => void checkForAppUpdate()} />
      {#if $updateState.version}
        <span class="text-sm text-zinc-300 light:text-zinc-700">{t("Versione ", $language)}{$updateState.version}{t(" disponibile.", $language)}{#if $updateState.aur}{t("Aggiorna tramite AUR.", $language)}{/if}</span>
        {#if !$updateState.aur}
          <Button label={$updateState.installing ? t("Installazione...", $language) : t("Installa e riavvia", $language)} disabled={$updateState.installing} onClick={() => void installAppUpdate()} />
        {/if}
      {:else if $updateState.checked}
        <span class="text-sm text-zinc-300 light:text-zinc-700">{t("Legio è aggiornato.", $language)}</span>
      {/if}
    </div>
    {#if $updateState.error}<ErrorBanner message={$updateState.error} />{/if}
  </SettingsGroup>

  {#if saved}
    <p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Impostazioni salvate.", $language)}</p>
  {/if}
</section>
