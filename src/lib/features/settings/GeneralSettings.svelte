<script lang="ts">
  import { t, language } from "../../i18n";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import Button from "../../components/ui/Button.svelte";
  import { pickGameDirectory } from "../../services/dialog";
  import { saveSettings, type Settings } from "../../services/local-state";
  import { configureSteamScanInterval } from "../../stores/bootstrap";
  import { settings, settingsError } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import { checkForAppUpdate, installAppUpdate, updateState } from "../../services/app-updater";

  const intervals = [5, 10, 15, 30, 60, 120];
  const options = $derived(intervals.map((minutes) => ({ value: String(minutes), label: t("{0} minuti", $language, [minutes]) })));
  const behaviors: { key: keyof Settings; label: string }[] = $derived([
    { key: "closeToTray", label: t("Riduci nell'area di notifica alla chiusura", $language) },
    { key: "hideOnGameStart", label: t("Nascondi Legio nell'area di notifica quando avvii un gioco", $language) },
    { key: "launchOnSystemStart", label: t("Avvia Legio all'accesso al sistema", $language) },
    { key: "launchMinimized", label: t("Avvia Legio ridotto nell'area di notifica", $language) },
    { key: "launchInLibrary", label: t("Apri Legio sulla Libreria", $language) },
    { key: "downloadNotifications", label: t("Notifica di sistema al termine del download", $language) },
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
      saved = true;
      return true;
    } catch (error) {
      settingsError.set(toMessage(error));
      return false;
    } finally {
      saving = false;
    }
  }

  async function setDownloadPath(): Promise<void> {
    try {
      const path = await pickGameDirectory($settings.data.downloadPath, t("Imposta la cartella per download e installazioni", $language));
      if (path !== null) await update({ downloadPath: path });
    } catch (error) {
      settingsError.set(toMessage(error));
    }
  }
</script>

<section class="flex flex-col gap-4">
  {#if $settingsError}
    <ErrorBanner message={$settingsError} />
  {/if}

  <SettingsGroup icon="translate" title={t("Lingua", $language)} description={t("I formati di data e numero seguono le impostazioni regionali del sistema.", $language)}>
    <SelectField id="ui-language" label={t("Lingua dell'interfaccia", $language)} value={$settings.data.language}
      options={[{ value: "system", label: t("Sistema", $language) }, { value: "it", label: "Italiano" }, { value: "en", label: "English" }]}
      disabled={saving} onChange={(value) => void selectLanguage(value)} />
  </SettingsGroup>
  <SettingsGroup icon="settings" title={t("Comportamento", $language)}>
    {#each behaviors as option (option.key)}
      <Toggle
        label={option.label}
        checked={Boolean($settings.data[option.key])}
        disabled={saving || (option.key === "launchMinimized" && !$settings.data.launchOnSystemStart)}
        onChange={(checked) => void update({ [option.key]: checked,
          ...(option.key === "launchOnSystemStart" && !checked ? { launchMinimized: false } : {}) })}
      />
    {/each}
  </SettingsGroup>

  <SettingsGroup
    icon="check"
    title={t("Integrità dei download: impostazione delicata", $language)}
    description={t("Controlla SHA-256 per i giochi verified. Disabilita questa protezione solo su computer di fascia bassa con prestazioni molto scarse: Legio non potrà rilevare archivi alterati o corrotti tramite hash. I giochi unverified vengono sempre estratti senza controllo SHA-256. La modifica si applica alle estrazioni successive.", $language)}
  >
    <Toggle
      label={t("Verifica SHA-256 dei giochi verified (consigliato)", $language)}
      checked={$settings.data.verifyVerifiedDownloads}
      disabled={saving || $settings.status === "loading" || $settings.status === "idle" || $settings.status === "error"}
      onChange={(checked) => void update({ verifyVerifiedDownloads: checked })}
    />
  </SettingsGroup>

  <SettingsGroup icon="reload" title={t("Aggiornamenti", $language)} description={t("Controlla automaticamente all'avvio se è disponibile una nuova versione di Legio.", $language)}>
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

  <SettingsGroup icon="folder" title={t("Cartella per download e installazioni", $language)}>
    <SettingsRow
      label={t("Percorso di destinazione", $language)}
      description={$settings.data.downloadPath ?? t("Cartella predefinita di Legio", $language)}
    >
      <div class="flex flex-col gap-2 sm:flex-row">
        <Button label={t("Imposta cartella...", $language)} variant="secondary" disabled={saving} onClick={() => void setDownloadPath()} />
        {#if $settings.data.downloadPath !== null}
          <Button label={t("Usa cartella predefinita", $language)} variant="secondary" disabled={saving} onClick={() => void update({ downloadPath: null })} />
        {/if}
      </div>
    </SettingsRow>
  </SettingsGroup>

  <SettingsGroup
    icon="store"
    title={t("Controllo Steam", $language)}
    description={t("Legio controlla Steam all'avvio e ripete il controllo all'intervallo scelto.", $language)}
  >
    <div class="max-w-56">
      <SelectField
        id="steam-library-poll-interval"
        label={t("Intervallo di controllo", $language)}
        value={String($settings.data.steamLibraryPollMinutes)}
        {options}
        disabled={saving || $settings.status === "loading" || $settings.status === "idle"}
        onChange={(value) => void selectInterval(value)}
      />
    </div>
  </SettingsGroup>

  {#if saved}
    <p class="text-sm text-emerald-300 light:text-emerald-700" role="status">{t("Impostazioni salvate.", $language)}</p>
  {/if}
</section>
