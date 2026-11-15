<script lang="ts" module>
  export const textFieldClass = "w-full rounded-lg border border-transparent bg-white/5 px-3 py-2 text-sm text-legio-fg outline-none transition-colors placeholder:text-legio-fg-subtle focus:border-legio-line-strong disabled:opacity-50 light:bg-white";
</script>

<script lang="ts">
  let {
    id,
    label,
    labelHidden = false,
    value = $bindable(""),
    type = "text",
    placeholder = "",
    hint = "",
    error = null,
    disabled = false,
    readonly = false,
    monospace = false,
    inputmode,
    oninput,
    rows = 4,
  }: {
    id: string;
    label: string;
    labelHidden?: boolean;
    value?: string;
    type?: "text" | "search" | "number" | "textarea";
    rows?: number;
    placeholder?: string;
    hint?: string;
    error?: string | null;
    disabled?: boolean;
    readonly?: boolean;
    monospace?: boolean;
    inputmode?: "text" | "numeric";
    oninput?: (value: string) => void;
  } = $props();

  const describedBy = $derived([error ? `${id}-error` : "", hint ? `${id}-hint` : ""].filter(Boolean).join(" ") || undefined);

  function changed(event: Event & { currentTarget: HTMLInputElement | HTMLTextAreaElement }): void {
    value = event.currentTarget.value;
    oninput?.(value);
  }
</script>

<div class="flex flex-col gap-1.5">
  <label for={id} class={labelHidden ? "sr-only" : "text-sm text-legio-fg-muted"}>{label}</label>
  {#if type === "textarea"}
    <textarea {id} {rows} {placeholder} {disabled} {readonly} {value} oninput={changed}
      aria-invalid={error ? true : undefined} aria-describedby={describedBy} class="{textFieldClass} font-mono"></textarea>
  {:else}
  <input
    {id}
    {type}
    {placeholder}
    {disabled}
    {readonly}
    {inputmode}
    value={value}
    oninput={changed}
    aria-invalid={error ? true : undefined}
    aria-describedby={describedBy}
    class="h-10 {textFieldClass} {monospace ? 'font-mono' : ''}"
  />
  {/if}
  {#if error}
    <p id={`${id}-error`} class="text-xs text-legio-error" role="alert">{error}</p>
  {/if}
  {#if hint}
    <p id={`${id}-hint`} class="text-xs text-legio-fg-subtle">{hint}</p>
  {/if}
</div>
