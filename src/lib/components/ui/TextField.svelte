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
    rows = 4,
  }: {
    id: string;
    label: string;
    value?: string;
    type?: "text" | "search" | "number" | "textarea";
    rows?: number;
    placeholder?: string;
    hint?: string;
    disabled?: boolean;
    inputmode?: "text" | "numeric";
    oninput?: (value: string) => void;
  } = $props();

  const fieldClass = "w-full rounded-lg border border-transparent bg-white/5 px-3 py-2 text-sm text-zinc-100 outline-none transition-colors placeholder:text-zinc-500 focus:border-white/40 disabled:opacity-50 light:bg-white light:text-zinc-900 light:focus:border-zinc-900/40";

  function changed(event: Event & { currentTarget: HTMLInputElement | HTMLTextAreaElement }): void {
    value = event.currentTarget.value;
    oninput?.(value);
  }
</script>

<div class="flex flex-col gap-1.5">
  <label for={id} class="text-sm text-zinc-400 light:text-zinc-600">{label}</label>
  {#if type === "textarea"}
    <textarea {id} {rows} {placeholder} {disabled} {value} oninput={changed}
      aria-describedby={hint ? `${id}-hint` : undefined} class="{fieldClass} font-mono"></textarea>
  {:else}
  <input
    {id}
    {type}
    {placeholder}
    {disabled}
    {inputmode}
    value={value}
    oninput={changed}
    aria-describedby={hint ? `${id}-hint` : undefined}
    class="h-10 {fieldClass}"
  />
  {/if}
  {#if hint}
    <p id={`${id}-hint`} class="text-xs text-zinc-500">{hint}</p>
  {/if}
</div>
