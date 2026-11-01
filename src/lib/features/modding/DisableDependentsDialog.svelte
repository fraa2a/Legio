<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Callout from "../../components/ui/Callout.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";

  let {
    open,
    title,
    dependents,
    onDisableOne,
    onDisableAll,
    onClose,
  }: {
    open: boolean;
    title: string;
    dependents: string[];
    onDisableOne: () => void;
    onDisableAll: () => void;
    onClose: () => void;
  } = $props();
</script>

<Dialog {open} title={t("Disattiva mod", $language)} {onClose}>
  {#snippet children(close)}
    <Callout tone="warning" variant="inline" message={t("Alcune mod attive hanno bisogno di {0}:", $language, [title])} items={dependents} />
    <div class="flex flex-wrap justify-end gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" onClick={close} />
      <Button label={t("Disattiva solo questa", $language)} variant="secondary" onClick={onDisableOne} />
      <Button label={t("Disattiva anche le dipendenti", $language)} variant="danger" onClick={onDisableAll} />
    </div>
  {/snippet}
</Dialog>
