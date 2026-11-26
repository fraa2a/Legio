<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLInputAttributes, HTMLTextareaAttributes } from "svelte/elements";
  import Icon from "./Icon.svelte";

  type ControlAttributes = Omit<HTMLInputAttributes & HTMLTextareaAttributes, "id" | "type" | "value" | "placeholder" | "disabled" | "readonly" | "inputmode" | "oninput" | "rows" | "class">;

  let {
    id,
    label,
    labelHidden = false,
    value = $bindable(""),
    element = $bindable(),
    type = "text",
    variant = "default",
    placeholder = "",
    hint = "",
    error = null,
    disabled = false,
    readonly = false,
    monospace = false,
    fill = false,
    inputmode,
    oninput,
    rows = 4,
    trailing,
    class: className = "",
    ...attributes
  }: ControlAttributes & {
    id: string;
    label: string;
    labelHidden?: boolean;
    value?: string;
    element?: HTMLInputElement | HTMLTextAreaElement;
    type?: "text" | "search" | "number" | "password" | "textarea";
    // Toolbar fields sit in the title bar on the glass surface.
    variant?: "default" | "toolbar";
    rows?: number;
    placeholder?: string;
    hint?: string;
    error?: string | null;
    disabled?: boolean;
    readonly?: boolean;
    monospace?: boolean;
    // A textarea that grows to fill a flex parent.
    fill?: boolean;
    inputmode?: "text" | "numeric";
    oninput?: (value: string) => void;
    // A small action inside the end of a search field.
    trailing?: Snippet;
    class?: string;
  } = $props();

  const describedBy = $derived([error ? `${id}-error` : "", hint ? `${id}-hint` : ""].filter(Boolean).join(" ") || undefined);
  const surfaceClass = $derived(variant === "toolbar"
    ? "rounded-card legio-glass"
    : "rounded-control border border-legio-line-control bg-legio-fill light:bg-white");
  const controlClass = $derived(`w-full px-3 text-sm text-legio-fg transition-colors placeholder:text-legio-fg-subtle disabled:cursor-not-allowed disabled:opacity-50 ${surfaceClass}`);

  function changed(event: Event & { currentTarget: HTMLInputElement | HTMLTextAreaElement }): void {
    value = event.currentTarget.value;
    oninput?.(value);
  }
</script>

<div class="flex flex-col gap-1.5 {fill ? 'min-h-0 flex-1' : ''} {className}">
  <label for={id} class={labelHidden ? "sr-only" : "text-sm font-medium text-legio-fg"}>{label}</label>
  {#if type === "textarea"}
    <textarea {...attributes} bind:this={element} {id} {rows} {placeholder} {disabled} {readonly} {value} oninput={changed}
      aria-invalid={error ? true : undefined} aria-describedby={describedBy}
      class="{controlClass} py-2 font-mono {fill ? 'min-h-0 flex-1 resize-none' : ''}"></textarea>
  {:else}
    <div class="relative">
      {#if type === "search"}
        <span class="pointer-events-none absolute inset-y-0 left-3 z-10 flex items-center text-legio-fg-subtle"><Icon name="search" /></span>
      {/if}
      <input
        {...attributes}
        bind:this={element}
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
        class="h-control-md {controlClass} {type === 'search' ? 'pl-9' : ''} {trailing ? 'pr-11' : ''} {monospace ? 'font-mono' : ''}"
      />
      {#if trailing}
        <span class="absolute inset-y-0 right-1 flex items-center">{@render trailing()}</span>
      {/if}
    </div>
  {/if}
  {#if error}
    <p id={`${id}-error`} class="select-text text-xs text-legio-error" data-selectable role="alert">{error}</p>
  {/if}
  {#if hint}
    <p id={`${id}-hint`} class="text-xs text-legio-fg-subtle">{hint}</p>
  {/if}
</div>
