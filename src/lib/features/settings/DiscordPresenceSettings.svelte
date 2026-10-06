<script lang="ts">
  import { t, language } from "../../i18n";
  import { saveSettings } from "../../services/local-state";
  import { defaultSettings, settings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";

  let applicationId = $derived($settings.data.discordPresence.applicationId || defaultSettings.discordPresence.applicationId);
  let saving = $state(false);
  let error = $state<string | null>(null);

  async function save(enabled: boolean, saveId = true): Promise<void> {
    saving = true;
    error = null;
    try {
      settings.set(await saveSettings({ ...$settings.data, discordPresence: {
        enabled, applicationId: saveId ? applicationId.trim() : $settings.data.discordPresence.applicationId,
      } }));
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<SettingsGroup icon="settings" title="Discord Rich Presence">
  <Toggle label={t("Mostra la tua attività su Discord", $language)}
    checked={$settings.data.discordPresence.enabled}
    disabled={saving || $settings.status !== "ready"}
    onChange={(enabled) => void save(enabled, enabled)} />
  <p class="text-xs text-zinc-500">{t("Condivide il nome del gioco e il tempo della sessione. Richiede Discord desktop aperto e la condivisione dell'attività abilitata su Discord.", $language)}</p>
  <TextField id="discord-application-id" label="Discord Application ID"
    bind:value={applicationId} inputmode="numeric" disabled={saving}
    hint={t("L'ID di Legio è già configurato. Modificalo solo per usare una tua applicazione Discord. Non inserire token o segreti.", $language)} />
  <Button label={t("Salva Application ID", $language)} variant="secondary"
    disabled={saving || applicationId.trim() === $settings.data.discordPresence.applicationId}
    onClick={() => void save($settings.data.discordPresence.enabled)} />
  {#if error}<ErrorBanner message={error} />{/if}
</SettingsGroup>
