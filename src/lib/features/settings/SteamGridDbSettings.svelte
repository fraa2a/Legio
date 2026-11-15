<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { clearSteamGridDbKey, hasSteamGridDbKey, setSteamGridDbKey } from "../../services/steamgriddb";
  import { canSaveKey } from "../library/steamgriddb";
  import { openGameLink } from "../library/open-link";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import SettingsGroup from "../../components/ui/SettingsGroup.svelte";
  import TextField from "../../components/ui/TextField.svelte";

  const KEY_PAGE = "https://www.steamgriddb.com/profile/preferences/api";

  let isSet = $state<boolean | null>(null);
  let replacing = $state(false);
  let input = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let destroyed = false;

  const editing = $derived(isSet === false || replacing);

  onMount(() => {
    void refresh();
    return () => { destroyed = true; };
  });

  async function refresh(): Promise<void> {
    try {
      const value = await hasSteamGridDbKey();
      if (!destroyed) isSet = value;
    } catch (cause) {
      if (!destroyed) { isSet = false; error = toMessage(cause); }
    }
  }

  async function run(action: () => Promise<void>): Promise<void> {
    busy = true;
    error = null;
    try {
      await action();
    } catch (cause) {
      if (!destroyed) error = toMessage(cause);
    } finally {
      if (!destroyed) busy = false;
    }
  }

  const save = () => run(async () => {
    await setSteamGridDbKey(input);
    input = "";
    replacing = false;
    isSet = true;
  });

  const remove = () => run(async () => {
    await clearSteamGridDbKey();
    isSet = false;
    replacing = false;
  });
</script>

<SettingsGroup icon="link" title="SteamGridDB">
  <p class="text-xs text-legio-fg-subtle">{t("Con una chiave API di SteamGridDB puoi scegliere copertine, hero e icone dalla scheda Personalizzazione di un gioco. La chiave resta nel portachiavi del sistema.", $language)}</p>
  {#if isSet === null}
    <p class="text-sm text-legio-fg-muted" role="status">{t("Caricamento...", $language)}</p>
  {:else if editing}
    <TextField id="steamgriddb-key" type="password" label={t("Chiave API", $language)} bind:value={input} disabled={busy} autocomplete="off" spellcheck={false} />
    <div class="flex flex-wrap gap-2">
      <Button label={t("Salva chiave", $language)} disabled={busy || !canSaveKey(input)} onClick={() => void save()} />
      {#if replacing}<Button label={t("Annulla", $language)} variant="secondary" disabled={busy} onClick={() => { replacing = false; input = ""; }} />{/if}
      <Button label={t("Ottieni una chiave", $language)} variant="ghost" onClick={() => void openGameLink(KEY_PAGE)} />
    </div>
  {:else}
    <p class="text-sm text-legio-fg">{t("Chiave impostata", $language)}</p>
    <div class="flex flex-wrap gap-2">
      <Button label={t("Sostituisci", $language)} variant="secondary" disabled={busy} onClick={() => { replacing = true; }} />
      <Button label={t("Rimuovi", $language)} variant="danger-soft" disabled={busy} onClick={() => void remove()} />
    </div>
  {/if}
  {#if error !== null}<ErrorBanner message={error} />{/if}
</SettingsGroup>
