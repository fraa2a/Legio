<script lang="ts">
  import { t, language } from "../../i18n";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
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

<ConfirmDialog
  {open}
  title={t("Annulla download", $language)}
  message={t("Annullare il download di {0}?", $language, [gameName])}
  cancelLabel={t("Indietro", $language)}
  confirmLabel={t("Annulla", $language)}
  tone="danger"
  {pending}
  error={actionError}
  onConfirm={() => void confirm()}
  {onClose}
/>
