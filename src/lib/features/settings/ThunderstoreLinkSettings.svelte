<script lang="ts">
  import { t, language } from "../../i18n";
  import { appInfo } from "../../stores/app-info";
  import { setModLinksEnabled } from "../../stores/mod-links";
  import { defaultSettings, settings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import ToggleSetting from "../../components/ui/ToggleSetting.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import { linkReleaseNote } from "../modding/mod-link-model";

  let saving = $state(false);
  let error = $state<string | null>(null);
  let note = $state<string | null>(null);

  // The scheme can only be claimed at runtime on Windows and Linux.
  const supported = $derived($appInfo.data.platform !== "macos");

  async function save(enabled: boolean): Promise<void> {
    saving = true;
    error = null;
    note = null;
    try {
      const handler = await setModLinksEnabled(enabled);
      note = handler === null ? null : linkReleaseNote(handler, $language);
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<SettingsGroup icon="globe" title={t("Link di Thunderstore", $language)}>
  <ToggleSetting label={t("Apri i link Mod Manager di Thunderstore con Legio", $language)}
    checked={$settings.data.thunderstoreLinksEnabled}
    defaultValue={defaultSettings.thunderstoreLinksEnabled}
    disabled={saving || !supported || $settings.status !== "ready"}
    onChange={(enabled) => void save(enabled)} />
  <p class="text-xs text-legio-fg-subtle">{t("Il pulsante \"Install with Mod Manager\" di Thunderstore apre Legio al posto del gestore mod attuale (per esempio r2modman) e propone di installare la mod in uno dei tuoi giochi. Disattivando l'opzione, Legio lascia il collegamento; r2modman lo riprende al prossimo avvio.", $language)}</p>
  {#if !supported}
    <p class="text-xs text-legio-fg-subtle">{t("Non disponibile su macOS.", $language)}</p>
  {/if}
  {#if note !== null}<Callout tone="warning" variant="inline" message={note} />{/if}
  {#if error}<ErrorBanner message={error} />{/if}
</SettingsGroup>
