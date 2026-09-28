<script lang="ts">
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import Button from "../../components/ui/Button.svelte";
  import { pickGameDirectory } from "../../services/dialog";
  import { saveSettings, type Settings } from "../../services/local-state";
  import { configureSteamScanInterval } from "../../stores/bootstrap";
  import { settings, settingsError } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";

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
  <div>
    <h3 class="text-xl font-semibold text-zinc-100 light:text-zinc-900">Generali</h3>
    <p class="mt-1 text-sm text-zinc-400 light:text-zinc-600">Configura l'avvio e il comportamento della finestra.</p>
  </div>
  <div class="max-w-xl flex flex-col gap-2">
    <p class="text-sm font-medium text-zinc-200 light:text-zinc-800">Cartella per download e installazioni</p>
    <p class="break-all text-sm text-zinc-400 light:text-zinc-600">{$settings.data.downloadPath ?? "Cartella predefinita di Legio"}</p>
    <div class="flex gap-2">
      <Button label="Imposta cartella..." variant="secondary" disabled={saving} onClick={() => void setDownloadPath()} />
      {#if $settings.data.downloadPath !== null}
        <Button label="Usa cartella predefinita" variant="secondary" disabled={saving} onClick={() => void update({ downloadPath: null })} />
      {/if}
    </div>
  </div>
  <div class="flex flex-col gap-3">
    {#each behaviors as option (option.key)}
      <label class="flex items-center gap-3 text-sm text-zinc-200 light:text-zinc-800">
        <input type="checkbox" class="accent-emerald-500" checked={Boolean($settings.data[option.key])}
          disabled={saving || (option.key === "launchMinimized" && !$settings.data.launchOnSystemStart)}
          onchange={(event) => void update({ [option.key]: event.currentTarget.checked,
            ...(option.key === "launchOnSystemStart" && !event.currentTarget.checked ? { launchMinimized: false } : {}) })} />
        {option.label}
      </label>
    {/each}
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
  {#if saved}<p class="text-sm text-emerald-300 light:text-emerald-700" role="status">Impostazioni salvate.</p>{/if}
</section>
