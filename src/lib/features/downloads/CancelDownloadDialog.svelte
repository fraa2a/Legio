<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import { cancelJob } from "../../stores/downloads";
  import { toMessage } from "../../utils/errors";

  let {
    open,
    jobId,
    gameName,
    onClose,
  }: {
    open: boolean;
    jobId: string;
    gameName: string;
    onClose: () => void;
  } = $props();

  let actionError = $state<string | null>(null);
  let pending = $state(false);

  async function confirm(): Promise<void> {
    if (pending) return;
    actionError = null;
    pending = true;
    try {
      await cancelJob(jobId);
      onClose();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pending = false;
    }
  }
</script>

<Dialog {open} title={t("Annulla download", $language)} {onClose}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Annullare il download di ", $language)}{gameName}?</p>
  {#if actionError}
    <p class="mt-3 text-sm text-red-300 light:text-red-700" role="alert">{actionError}</p>
  {/if}
  <div class="mt-5 flex justify-end gap-2">
    <Button label={t("Indietro", $language)} variant="secondary" onClick={onClose} />
    <Button label={t("Annulla", $language)} variant="danger" disabled={pending} onClick={() => void confirm()} />
  </div>
</Dialog>
