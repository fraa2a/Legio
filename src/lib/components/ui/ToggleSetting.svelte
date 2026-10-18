<script lang="ts">
  import { language, t } from "../../i18n";
  import Toggle from "./Toggle.svelte";
  import ResetSetting from "./ResetSetting.svelte";
  let { label, checked, defaultValue = false, disabled = false, onChange }: {
    label: string; checked: boolean; defaultValue?: boolean; disabled?: boolean;
    onChange: (value: boolean) => void;
  } = $props();
</script>

<div class="flex items-center gap-2">
  <div class="min-w-0 flex-1"><Toggle {label} {checked} {disabled} {onChange} /></div>
  <ResetSetting label={t("Ripristina {0}", $language, [label])} disabled={disabled || checked === defaultValue} onClick={() => onChange(defaultValue)} />
</div>
