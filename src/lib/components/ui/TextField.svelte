<script lang="ts">
  let {
    id,
    label,
    value = $bindable(""),
    type = "text",
    placeholder = "",
    hint = "",
    disabled = false,
    inputmode,
    oninput,
  }: {
    id: string;
    label: string;
    value?: string;
    type?: "text" | "search" | "number";
    placeholder?: string;
    hint?: string;
    disabled?: boolean;
    inputmode?: "text" | "numeric";
    oninput?: (value: string) => void;
  } = $props();
</script>

<div class="flex flex-col gap-1.5">
  <label for={id} class="text-sm text-zinc-400 light:text-zinc-600">{label}</label>
  <input
    {id}
    {type}
    {placeholder}
    {disabled}
    {inputmode}
    value={value}
    oninput={(event) => {
      value = event.currentTarget.value;
      oninput?.(value);
    }}
    aria-describedby={hint ? `${id}-hint` : undefined}
    class="w-full rounded-lg bg-white/5 px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 disabled:opacity-50 light:bg-white light:text-zinc-900"
  />
  {#if hint}
    <p id={`${id}-hint`} class="text-xs text-zinc-500">{hint}</p>
  {/if}
</div>
