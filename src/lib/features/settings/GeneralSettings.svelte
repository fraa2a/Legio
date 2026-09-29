<script lang="ts">
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import Button from "../../components/ui/Button.svelte";
  import { pickGameDirectory } from "../../services/dialog";
  import { saveSettings, type Settings } from "../../services/local-state";
  import { configureSteamScanInterval } from "../../stores/bootstrap";
  import { settings, settingsError } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import { checkForAppUpdate, installAppUpdate, updateState } from "../../services/app-updater";

  const intervals = [5, 10, 15, 30, 60, 120];
  const options = intervals.map((minutes) => ({ value: String(minutes), label: `${minutes} minuti` }));
  const behaviors: { key: keyof Settings; label: string }[] = [
    { key: "closeToTray", label: "Riduci nell'area di notifica alla chiusura" },
    { key: "hideOnGameStart", label: "Nascondi Legio nell'area di notifica quando avvii un gioco" },
    { key: "launchOnSystemStart", label: "Avvia Legio all'accesso al sistema" },
    { key: "launchMinimized", label: "Avvia Legio ridotto nell'area di notifica" },
    { key: "launchInLibrary", label: "Apri Legio sulla Libreria" },
    { key: "downloadNotifications", label: "Notifica di sistema al termine del download" },
  ];
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
      const path = await pickGameDirectory($settings.data.downloadPath, "Imposta la cartella per download e installazioni");
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

  <SettingsGroup title="Comportamento">
    {#each behaviors as option (option.key)}
      <label class="flex cursor-pointer items-center justify-between gap-6">
        <span class="text-sm text-zinc-100 light:text-zinc-900">{option.label}</span>
        <input
          type="checkbox"
          class="size-4 accent-white"
          checked={Boolean($settings.data[option.key])}
          disabled={saving || (option.key === "launchMinimized" && !$settings.data.launchOnSystemStart)}
          onchange={(event) => void update({ [option.key]: event.currentTarget.checked,
            ...(option.key === "launchOnSystemStart" && !event.currentTarget.checked ? { launchMinimized: false } : {}) })}
        />
      </label>
    {/each}
  </SettingsGroup>

  <SettingsGroup title="Aggiornamenti" description="Controlla automaticamente all'avvio se è disponibile una nuova versione di Legio.">
    <div class="flex flex-wrap items-center gap-3">
      <Button label={$updateState.checking ? "Controllo..." : "Controlla aggiornamenti"} variant="secondary" disabled={$updateState.checking || $updateState.installing} onClick={() => void checkForAppUpdate()} />
      {#if $updateState.version}
        <span class="text-sm text-zinc-300 light:text-zinc-700">Versione {$updateState.version} disponibile.{#if $updateState.aur} Aggiorna tramite AUR.{/if}</span>
        {#if !$updateState.aur}
          <Button label={$updateState.installing ? "Installazione..." : "Installa e riavvia"} disabled={$updateState.installing} onClick={() => void installAppUpdate()} />
        {/if}
      {:else if $updateState.checked}
        <span class="text-sm text-zinc-300 light:text-zinc-700">Legio è aggiornato.</span>
      {/if}
    </div>
    {#if $updateState.error}<ErrorBanner message={$updateState.error} />{/if}
  </SettingsGroup>

  <SettingsGroup title="Cartella per download e installazioni">
    <SettingsRow
      label="Percorso di destinazione"
      description={$settings.data.downloadPath ?? "Cartella predefinita di Legio"}
    >
      <div class="flex flex-col gap-2 sm:flex-row">
        <Button label="Imposta cartella..." variant="secondary" disabled={saving} onClick={() => void setDownloadPath()} />
        {#if $settings.data.downloadPath !== null}
          <Button label="Usa cartella predefinita" variant="secondary" disabled={saving} onClick={() => void update({ downloadPath: null })} />
        {/if}
      </div>
    </SettingsRow>
  </SettingsGroup>

  <SettingsGroup
    title="Controllo Steam"
    description="Legio controlla Steam all'avvio e ripete il controllo all'intervallo scelto."
  >
    <div class="max-w-56">
      <SelectField
        id="steam-library-poll-interval"
        label="Intervallo di controllo"
        value={String($settings.data.steamLibraryPollMinutes)}
        {options}
        disabled={saving || $settings.status === "loading" || $settings.status === "idle"}
        onChange={(value) => void selectInterval(value)}
      />
    </div>
  </SettingsGroup>

  {#if saved}
    <p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Impostazioni salvate.</p>
  {/if}
</section>
