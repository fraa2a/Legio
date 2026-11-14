<script lang="ts">
  import { t, language } from "../../i18n";
  import type { ConfigEntry } from "../../services/mod-config";
  import Badge from "../../components/ui/Badge.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import SelectField from "../../components/ui/SelectField.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { choiceOptions, entryControl, formatBoolean, isIntegerEntry, rangeHint, validateEntry } from "./mod-config-model";

  let { id, entry, value, onChange }: {
    id: string;
    entry: ConfigEntry;
    value: string;
    onChange: (value: string) => void;
  } = $props();

  const control = $derived(entryControl(entry));
  const error = $derived(validateEntry(entry, value, $language));
  const changed = $derived(value !== entry.value);
  const canReset = $derived(entry.defaultValue !== null && value !== entry.defaultValue);
  const range = $derived(rangeHint(entry, $language));
</script>

<div class="flex flex-col gap-2 rounded-lg bg-legio-fill p-3 [contain-intrinsic-size:auto_6rem] [content-visibility:auto]">
  <div class="flex items-center justify-between gap-3">
    <div class="min-w-0 flex-1">
      {#if control === "boolean"}
        <Toggle label={entry.key} checked={value.trim().toLowerCase() === "true"} onChange={(checked) => onChange(formatBoolean(checked))} />
      {:else}
        <p class="flex items-center gap-2 text-sm font-medium break-words text-legio-fg">
          {entry.key}
          {#if changed}<span class="size-1.5 shrink-0 rounded-full bg-legio-info" title={t("Modificato", $language)}></span>{/if}
        </p>
      {/if}
    </div>
    <div class="flex shrink-0 items-center gap-2">
      {#if entry.settingType}<Badge title={entry.settingType} />{/if}
      <IconButton
        size="sm"
        label={t("Ripristina il valore predefinito", $language)}
        title={t("Ripristina il valore predefinito", $language)}
        disabled={!canReset}
        onClick={() => entry.defaultValue !== null && onChange(entry.defaultValue)}
      ><Icon name="reload" size="size-4" /></IconButton>
    </div>
  </div>
  {#if entry.description}
    <p class="text-xs whitespace-pre-line text-legio-fg-muted">{entry.description}</p>
  {/if}
  {#if control === "choice"}
    <SelectField {id} label={entry.key} labelHidden value={value} options={choiceOptions(entry, value)} onChange={onChange} />
  {:else if control === "number"}
    <TextField {id} label={entry.key} labelHidden type="number" inputmode={isIntegerEntry(entry) ? "numeric" : "text"} {value} {error} oninput={onChange} />
  {:else if control === "text"}
    <TextField {id} label={entry.key} labelHidden {value} {error} oninput={onChange} />
  {/if}
  {#if entry.defaultValue !== null || range}
    <p class="text-xs text-legio-fg-subtle">
      {#if entry.defaultValue !== null}{t("Predefinito: {0}", $language, [entry.defaultValue === "" ? t("(vuoto)", $language) : entry.defaultValue])}{/if}
      {#if entry.defaultValue !== null && range}&middot;{/if}
      {#if range}{range}{/if}
    </p>
  {/if}
</div>
