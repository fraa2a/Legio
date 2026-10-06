<script lang="ts">
  import { t, language } from "../../i18n";
  import ResetSetting from "../../components/ui/ResetSetting.svelte";

  let {
    id,
    label,
    value,
    min,
    max,
    unit,
    resetValue,
    step = 1,
    disabled = false,
    onChange,
  }: {
    id: string;
    label: string;
    value: number;
    min: number;
    max: number;
    unit: string;
    resetValue: number;
    step?: number;
    disabled?: boolean;
    onChange: (value: number) => void;
  } = $props();
</script>

<div class="flex min-w-0 flex-col gap-3 rounded-xl bg-black/10 p-3 light:bg-white/70">
  <div class="flex items-center justify-between gap-2">
    <label for={id} class="min-w-0 truncate text-sm font-medium text-zinc-200 light:text-zinc-800">{label}</label>
    <div class="flex shrink-0 items-center gap-1">
      <output for={id} class="text-xs tabular-nums text-zinc-400 light:text-zinc-600">{value}<span class="ml-0.5">{unit}</span></output>
      <ResetSetting label={t("Ripristina {0}", $language, [label])} {disabled} onClick={() => onChange(resetValue)} />
    </div>
  </div>
  <input
    {id}
    type="range"
    {min}
    {max}
    {step}
    {value}
    {disabled}
    aria-label={label}
    class="w-full disabled:cursor-not-allowed disabled:opacity-40"
    oninput={(event) => onChange(Number(event.currentTarget.value))}
  />
</div>
