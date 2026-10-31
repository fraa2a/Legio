<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { t, language } from "../../i18n";
  import { pickModProfileExportPath } from "../../services/dialog";
  import { exportModProfileFile, shareModProfileCode } from "../../services/mod-profile-exchange";
  import type { ModProfile } from "../../services/modding";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Spinner from "../../components/ui/Spinner.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { exportFileName, fileNameOf } from "./mod-profile-exchange-model";

  let { open, gameId, profile, onClose }: {
    open: boolean;
    gameId: string;
    profile: ModProfile;
    onClose: () => void;
  } = $props();

  const copiedDuration = 2000;

  let exporting = $state(false);
  let exportedPath = $state<string | null>(null);
  let exportError = $state<string | null>(null);
  let generating = $state(false);
  let code = $state<string | null>(null);
  let codeError = $state<string | null>(null);
  let copied = $state(false);
  let copyError = $state<string | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  const busy = $derived(exporting || generating);

  $effect(() => {
    if (open) untrack(reset);
  });

  onDestroy(() => clearTimeout(copiedTimer));

  function reset(): void {
    clearTimeout(copiedTimer);
    exporting = false;
    exportedPath = null;
    exportError = null;
    generating = false;
    code = null;
    codeError = null;
    copied = false;
    copyError = null;
  }

  async function exportFile(): Promise<void> {
    if (busy) return;
    exportError = null;
    exportedPath = null;
    exporting = true;
    try {
      const destination = await pickModProfileExportPath(exportFileName(profile.name));
      if (destination === null) return;
      await exportModProfileFile(gameId, profile.id, destination);
      exportedPath = destination;
    } catch (reason) {
      exportError = toMessage(reason);
    } finally {
      exporting = false;
    }
  }

  async function generateCode(): Promise<void> {
    if (busy) return;
    codeError = null;
    code = null;
    copied = false;
    copyError = null;
    generating = true;
    try {
      code = await shareModProfileCode(gameId, profile.id);
    } catch (reason) {
      codeError = toMessage(reason);
    } finally {
      generating = false;
    }
  }

  async function copyCode(): Promise<void> {
    if (code === null) return;
    copyError = null;
    try {
      await navigator.clipboard.writeText(code);
      copied = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), copiedDuration);
    } catch {
      copyError = t("Impossibile copiare negli appunti. Seleziona il codice e copialo a mano.", $language);
    }
  }
</script>

<Dialog {open} title={t("Condividi profilo", $language)} dismissible={!busy} {onClose}>
  {#snippet children(close)}
    <p class="text-sm text-zinc-300 light:text-zinc-700">
      {t("Profilo \"{0}\". Altri giocatori possono importarlo da un file o da un codice.", $language, [profile.name])}
    </p>
    <Callout variant="inline" message={t("Le mod locali non sono incluse: chi importa il profilo riceve solo le mod di Thunderstore e i file di configurazione.", $language)} />

    <section class="flex flex-col gap-3 rounded-lg bg-white/5 p-3 light:bg-zinc-900/5" aria-label={t("Esporta in un file", $language)}>
      <div>
        <h3 class="text-sm font-semibold text-zinc-100 light:text-zinc-900">{t("Esporta in un file", $language)}</h3>
        <p class="mt-0.5 text-xs text-zinc-400 light:text-zinc-600">{t("Crea un file .r2z compatibile con r2modman.", $language)}</p>
      </div>
      {#if exportError !== null}<ErrorBanner message={exportError} />{/if}
      {#if exportedPath !== null}
        <Callout tone="success" variant="inline" message={t("Profilo esportato in {0}", $language, [fileNameOf(exportedPath)])} />
      {/if}
      <div>
        <Button label={exporting ? t("Esportazione...", $language) : t("Esporta file...", $language)} variant="secondary" disabled={busy} onClick={() => void exportFile()}>
          {#if exporting}<Spinner size="sm" />{/if}
        </Button>
      </div>
    </section>

    <section class="flex flex-col gap-3 rounded-lg bg-white/5 p-3 light:bg-zinc-900/5" aria-label={t("Codice di condivisione", $language)}>
      <div>
        <h3 class="text-sm font-semibold text-zinc-100 light:text-zinc-900">{t("Codice di condivisione", $language)}</h3>
        <p class="mt-0.5 text-xs text-zinc-400 light:text-zinc-600">{t("Carica il profilo su Thunderstore e genera un codice da incollare. Il codice scade dopo un po' di tempo.", $language)}</p>
      </div>
      <Callout tone="warning" variant="inline" message={t("Il profilo e tutti i suoi file di configurazione vengono caricati su Thunderstore in modo pubblico: chiunque abbia il codice può scaricarli. Non condividere configurazioni con dati personali.", $language)} />
      {#if codeError !== null}<ErrorBanner message={codeError} />{/if}
      {#if code !== null}
        <div class="flex items-center gap-2">
          <div class="min-w-0 flex-1">
            <TextField id="mod-profile-share-code" label={t("Codice di condivisione", $language)} labelHidden readonly monospace value={code} />
          </div>
          <Button label={copied ? t("Copiato", $language) : t("Copia", $language)} variant="secondary" onClick={() => void copyCode()}>
            {#if copied}<Icon name="check" size="size-4 text-legio-success" />{/if}
          </Button>
        </div>
        {#if copyError !== null}<ErrorBanner message={copyError} />{/if}
      {/if}
      <div>
        <Button label={generating ? t("Generazione...", $language) : code === null ? t("Genera codice", $language) : t("Genera un nuovo codice", $language)} variant="secondary" disabled={busy} onClick={() => void generateCode()}>
          {#if generating}<Spinner size="sm" />{/if}
        </Button>
      </div>
    </section>

    <div class="flex justify-end"><Button label={t("Chiudi", $language)} disabled={busy} onClick={close} /></div>
  {/snippet}
</Dialog>
