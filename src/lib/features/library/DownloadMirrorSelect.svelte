<script lang="ts">
  import { t, language } from "../../i18n";
  import SelectField from "../../components/ui/SelectField.svelte";

  let { uris, value = $bindable("auto"), disabled = false }: {
    uris: string[];
    value?: string;
    disabled?: boolean;
  } = $props();

  const options = $derived([
    { value: "auto", label: t("Automatico", $language) },
    ...uris.map(uri => ({ value: uri, label: uri.startsWith("magnet:") ? "Torrent" : new URL(uri).hostname })),
  ]);

  $effect(() => {
    if (value !== "auto" && !uris.includes(value)) value = "auto";
  });
</script>

{#if uris.length > 1}
  <SelectField id="download-mirror" label={t("Collegamento di download", $language)} bind:value {options} {disabled} variant="hero" class="w-[260px] min-w-0 shrink-0" />
{/if}
