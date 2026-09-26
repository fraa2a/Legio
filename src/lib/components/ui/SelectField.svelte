<script lang="ts">
  let {
    id,
    label,
    value = $bindable(),
    options,
    disabled = false,
    class: className = "",
  }: {
    id: string;
    label: string;
    value: string;
    options: { value: string; label: string }[];
    disabled?: boolean;
    class?: string;
  } = $props();
</script>

<div class="flex flex-col gap-1.5 {className}">
  <label for={id} class="text-sm text-zinc-400 light:text-zinc-600">{label}</label>
  <select
    {id}
    {disabled}
    {value}
    onchange={(event) => (value = event.currentTarget.value)}
    class="h-10 w-full rounded-lg bg-white/5 px-3 py-2 text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"
  >
    {#each options as option (option.value)}
      <option value={option.value}>{option.label}</option>
    {/each}
  </select>
</div>
