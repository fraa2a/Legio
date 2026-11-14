<script lang="ts">
  let {
    name,
    label,
    labelHidden = false,
    options,
    value,
    disabled = false,
    onChange,
    class: className = "",
  }: {
    name: string;
    label: string;
    labelHidden?: boolean;
    options: { value: string; label: string }[];
    value: string | null;
    disabled?: boolean;
    onChange: (value: string) => void;
    class?: string;
  } = $props();
</script>

<fieldset {disabled} class="min-w-0 {className}">
  <legend class={labelHidden ? "sr-only" : "mb-1.5 text-sm font-medium text-legio-fg"}>{label}</legend>
  <div class="flex gap-1 rounded-xl bg-black/20 p-1 light:bg-zinc-900/5">
    {#each options as option (option.value)}
      <label class="min-w-0 flex-1 cursor-pointer has-disabled:cursor-not-allowed has-disabled:opacity-50">
        <input
          class="peer sr-only"
          type="radio"
          {name}
          value={option.value}
          checked={value === option.value}
          onchange={() => onChange(option.value)}
        />
        <span class="block truncate rounded-lg px-3 py-1.5 text-center text-sm text-legio-fg-muted transition-colors hover:text-legio-fg peer-checked:bg-white/15 peer-checked:text-legio-fg peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-legio-accent light:peer-checked:bg-white light:peer-checked:shadow-sm">{option.label}</span>
      </label>
    {/each}
  </div>
</fieldset>
