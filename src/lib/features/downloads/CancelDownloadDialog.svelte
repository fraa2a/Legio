<script lang="ts">
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

<Dialog {open} title="Annulla download" {onClose}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">Annullare il download di {gameName}?</p>
  {#if actionError}
    <p class="mt-3 text-sm text-red-300 light:text-red-700" role="alert">{actionError}</p>
  {/if}
  <div class="mt-5 flex justify-end gap-2">
    <Button label="Indietro" variant="secondary" onClick={onClose} />
    <Button label="Annulla" variant="danger" disabled={pending} onClick={() => void confirm()} />
  </div>
</Dialog>
