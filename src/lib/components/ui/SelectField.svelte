<script lang="ts">
  let {
    id,
    label,
    value = $bindable(),
    options,
    disabled = false,
  }: {
    id: string;
    label: string;
    value: string;
    options: { value: string; label: string }[];
    disabled?: boolean;
  } = $props();
</script>

<div class="flex flex-col gap-1.5">
  <label for={id} class="text-sm text-zinc-400 light:text-zinc-600">{label}</label>
  <select
    {id}
    {disabled}
    {value}
    onchange={(event) => (value = event.currentTarget.value)}
    class="rounded-lg bg-white/5 px-3 py-2 text-sm text-zinc-100 disabled:opacity-50 light:bg-white light:text-zinc-900"
  >
    {#each options as option (option.value)}
      <option value={option.value}>{option.label}</option>
    {/each}
  </select>
</div>
