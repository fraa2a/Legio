<script lang="ts">
  import { t, language } from "../../i18n";
  import { settings, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";

  let saving = $state(false);
  let error = $state<string | null>(null);

  async function save(enabled: boolean): Promise<void> {
    saving = true;
    error = null;
    try {
      await updateSettings({ discordPresence: { enabled } });
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
    onChange={(enabled) => void save(enabled)} />
  <p class="text-xs text-zinc-500">{t("Condivide il nome del gioco e il tempo della sessione. Richiede Discord desktop aperto e la condivisione dell'attività abilitata su Discord.", $language)}</p>
  {#if error}<ErrorBanner message={error} />{/if}
</SettingsGroup>
