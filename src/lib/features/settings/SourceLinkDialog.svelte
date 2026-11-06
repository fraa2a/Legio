<script lang="ts">
  import { t, language } from "../../i18n";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import { addSource } from "../../stores/source";
  import { currentSourceLink, dismissSourceLink } from "../../stores/source-links";

  let pending = $state(false);

  async function confirm(url: string): Promise<void> {
    if (pending) return;
    pending = true;
    try {
      // Failures are reported by the Sources settings page.
      await addSource(url);
    } finally {
      pending = false;
      dismissSourceLink();
    }
  }
</script>

{#if $currentSourceLink !== null}
  {@const url = $currentSourceLink}
  <ConfirmDialog
    title={t("Aggiungere questa sorgente?", $language)}
    message={t("Un collegamento chiede di aggiungere una sorgente di download. Aggiungila solo se ti fidi di chi la pubblica: può proporre qualsiasi file da scaricare.", $language)}
    confirmLabel={t("Aggiungi sorgente", $language)}
    {pending}
    onConfirm={() => void confirm(url)}
    onClose={dismissSourceLink}
  >
    <p class="break-all rounded-lg bg-zinc-900 p-3 font-mono text-xs text-zinc-200 light:bg-zinc-100 light:text-zinc-800">{url}</p>
  </ConfirmDialog>
{/if}
