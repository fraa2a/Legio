<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
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
    <p class="text-sm text-zinc-300 light:text-zinc-700">{t("Alcune mod attive hanno bisogno di {0}:", $language, [title])}</p>
    <ul class="list-disc space-y-0.5 pl-5 text-sm text-zinc-300 light:text-zinc-700">
      {#each dependents as dependent (dependent)}<li>{dependent}</li>{/each}
    </ul>
    <div class="flex flex-wrap justify-end gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" onClick={close} />
      <Button label={t("Disattiva solo questa", $language)} variant="secondary" onClick={onDisableOne} />
      <Button label={t("Disattiva anche le dipendenti", $language)} variant="danger" onClick={onDisableAll} />
    </div>
  {/snippet}
</Dialog>
