<script lang="ts">
  import { t, language } from "../../i18n";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import Tooltip from "../../components/ui/Tooltip.svelte";
  import { enableQueueShutdown, queueShutdown, queueShutdownCountdown } from "../../stores/queue-shutdown";
  import { toMessage } from "../../utils/errors";

  let { hasPendingWork }: { hasPendingWork: boolean } = $props();

  let confirming = $state(false);
  let pending = $state(false);
  let actionError = $state<string | null>(null);

  const phase = $derived($queueShutdown.phase);
  let enabled = $derived(phase !== "off");

  async function apply(enabled: boolean): Promise<void> {
    if (pending) return;
    pending = true;
    actionError = null;
    try {
      await enableQueueShutdown(enabled);
      confirming = false;
    } catch (error) {
      actionError = toMessage(error);
      enabled = phase !== "off";
    } finally {
      pending = false;
    }
  }
</script>

<Panel title={t("Spegnimento automatico", $language)}>
  <div class="flex flex-col gap-3">
    {#if $queueShutdown.error !== null}
      <ErrorBanner message={t("Impossibile spegnere il computer: {0}", $language, [$queueShutdown.error])} />
    {/if}
    {#if actionError !== null && !confirming}
      <ErrorBanner message={actionError} />
    {/if}

    {#if phase === "countdown"}
      <ErrorBanner
        tone="warning"
        message={t("Il computer si spegnerà tra {0} secondi.", $language, [$queueShutdownCountdown ?? 0])}
      />
    {:else if phase === "waiting"}
      <p class="text-sm text-legio-fg" role="status">
        {t("Il computer si spegnerà 30 secondi dopo il completamento di tutta la coda.", $language)}
      </p>
    {/if}

    {#snippet toggle()}
      <Toggle
        label={t("Spegnimento automatico", $language)}
        labelHidden
        bind:checked={enabled}
        disabled={(phase === "off" && !hasPendingWork) || pending}
        onChange={(value) => { if (value) { actionError = null; confirming = true; } else void apply(false); }}
      />
    {/snippet}
    {#if phase === "off" && !hasPendingWork}
      <Tooltip text={t("Disponibile quando ci sono download in coda.", $language)}>{@render toggle()}</Tooltip>
    {:else}
      {@render toggle()}
    {/if}
  </div>
</Panel>

<ConfirmDialog
  open={confirming}
  title={t("Spegnere il computer a fine coda?", $language)}
  message={t("Dopo che tutti i download sono stati estratti e installati, il computer si spegnerà dopo 30 secondi. Salva il lavoro aperto in altre applicazioni. Potrai annullare in qualsiasi momento. Se un download fallisce, resta in coda e si passa al successivo. L'impostazione vale solo fino alla chiusura di Legio.", $language)}
  cancelLabel={t("Indietro", $language)}
  confirmLabel={t("Attiva", $language)}
  {pending}
  error={actionError}
  onConfirm={() => void apply(true)}
  onClose={() => { confirming = false; enabled = phase !== "off"; }}
/>
