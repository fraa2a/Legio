<script lang="ts">
  let {
    label,
    checked = $bindable(false),
    disabled = false,
    onChange,
  }: {
    label: string;
    checked?: boolean;
    disabled?: boolean;
    onChange?: (checked: boolean) => void;
  } = $props();
</script>

<label class="flex items-center justify-between gap-6 {disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer'}">
  <span class="min-w-0 text-sm text-zinc-100 light:text-zinc-900">{label}</span>
  <span
    class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full bg-white/15 transition-colors duration-200 has-checked:bg-legio-accent has-focus-visible:ring-2 has-focus-visible:ring-legio-accent/60 light:bg-zinc-300"
  >
    <input
      type="checkbox"
      class="peer sr-only"
      {disabled}
      {checked}
      onchange={(event) => {
        checked = event.currentTarget.checked;
        onChange?.(checked);
      }}
    />
    <span
      aria-hidden="true"
      class="pointer-events-none absolute top-0.5 left-0.5 size-5 rounded-full bg-white shadow-sm transition-transform duration-200 peer-checked:translate-x-5 peer-checked:bg-legio-accent-text"
    ></span>
  </span>
</label>
