<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import { closeReleaseNotes, openedNotes, type ReleaseNotes } from "../../stores/update-notes";

  // Kept after closing so the content stays during the exit animation.
  let shown: ReleaseNotes | null = $state(null);

  $effect(() => {
    if ($openedNotes !== null) shown = $openedNotes;
  });
</script>

<Dialog open={$openedNotes !== null} title={t("Cosa c'è di nuovo in Legio {0}", $language, [shown?.version ?? ""])} size="wide" onClose={closeReleaseNotes}>
  {#snippet children(dismiss)}
    <!-- Notes come from a remote manifest: plain text only. -->
    <pre class="max-h-[50vh] overflow-y-auto rounded-control bg-legio-fill p-4 font-sans text-sm whitespace-pre-wrap text-legio-fg">{shown?.notes ?? ""}</pre>
    <div class="flex justify-end">
      <Button label={t("Chiudi", $language)} variant="secondary" onClick={dismiss} />
    </div>
  {/snippet}
</Dialog>
