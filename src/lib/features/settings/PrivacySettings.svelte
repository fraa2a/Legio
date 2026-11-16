<script lang="ts">
  import { t, language } from "../../i18n";
  import { settings, updateSettings } from "../../stores/settings";
  import { toMessage } from "../../utils/errors";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import SettingsRow from "../../components/ui/SettingsRow.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { isNetworkSwitchOn, networkDestinations, networkSwitchChange, type NetworkDestination } from "./network-destinations";

  let saving = $state(false);
  let error = $state<string | null>(null);

  const kindLabel = { essential: "Necessario", "on-demand": "Solo se usi la funzione" } as const;

  async function change(destination: NetworkDestination, enabled: boolean): Promise<void> {
    if (!destination.switch) return;
    saving = true;
    error = null;
    try {
      await updateSettings(networkSwitchChange(destination.switch, enabled));
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<section class="flex flex-col gap-4">
  <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Ecco gli indirizzi che Legio contatta e perché. Legio non invia telemetria né statistiche d'uso.", $language)}</p>
  {#if error}<ErrorBanner message={error} />{/if}
  {#each networkDestinations as destination (destination.id)}
    <SettingsGroup icon="globe" title={t(destination.name, $language)}>
      <SettingsRow label={t(destination.purpose, $language)} description={t(destination.hosts, $language)}>
        {#if destination.switch}
          {@const name = destination.switch}
          <Toggle label={t(destination.name, $language)} checked={isNetworkSwitchOn($settings.data, name)}
            disabled={saving || $settings.status !== "ready"}
            onChange={(enabled) => void change(destination, enabled)} />
        {:else if destination.kind !== "switch"}
          <span class="text-xs text-zinc-400 light:text-zinc-600">{t(kindLabel[destination.kind], $language)}</span>
        {/if}
      </SettingsRow>
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Quando:", $language)} {t(destination.when, $language)}</p>
      <p class="text-xs text-zinc-400 light:text-zinc-600">{t("Dati inviati:", $language)} {t(destination.sends, $language)}</p>
    </SettingsGroup>
  {/each}
</section>
