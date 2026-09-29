<script lang="ts">
  import { tick } from "svelte";

  let {
    id,
    label,
    value = $bindable(),
    options,
    disabled = false,
    onChange,
    class: className = "",
  }: {
    id: string;
    label: string;
    value: string;
    options: { value: string; label: string }[];
    disabled?: boolean;
    onChange?: (value: string) => void;
    class?: string;
  } = $props();

  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let open = $state(false);
  let optionsTop = $state(0);
  let optionsLeft = $state(0);
  let optionsWidth = $state(0);
  let optionsMaxHeight = $state(256);
  const selectedLabel = $derived(options.find((option) => option.value === value)?.label ?? value);

  $effect(() => {
    if (disabled) open = false;
  });

  $effect(() => {
    if (!open) return;
    const closeOnScroll = (event: Event) => {
      if (!(event.target instanceof Node && root.contains(event.target))) open = false;
    };
    const closeOnResize = () => (open = false);
    window.addEventListener("scroll", closeOnScroll, true);
    window.addEventListener("resize", closeOnResize);
    return () => {
      window.removeEventListener("scroll", closeOnScroll, true);
      window.removeEventListener("resize", closeOnResize);
    };
  });

  async function showOptions(): Promise<void> {
    if (disabled) return;
    open = true;
    await tick();
    const listbox = root.querySelector<HTMLDivElement>("[role='listbox']");
    if (listbox === null) return;

    const triggerRect = trigger.getBoundingClientRect();
    const gap = 8;
    const spaceAbove = Math.max(0, triggerRect.top - gap);
    const spaceBelow = Math.max(0, window.innerHeight - triggerRect.bottom - gap);
    const desiredHeight = Math.min(256, listbox.scrollHeight);
    const above = spaceBelow < desiredHeight && spaceAbove > spaceBelow;
    optionsMaxHeight = Math.floor(Math.min(desiredHeight, above ? spaceAbove : spaceBelow));
    optionsTop = above ? triggerRect.top - gap - optionsMaxHeight : triggerRect.bottom + gap;
    optionsLeft = triggerRect.left;
    optionsWidth = triggerRect.width;

    const selected = root.querySelector<HTMLButtonElement>('[role="option"][aria-selected="true"]');
    (selected ?? root.querySelector<HTMLButtonElement>('[role="option"]'))?.focus();
  }

  function choose(next: string): void {
    if (onChange) onChange(next);
    else value = next;
    open = false;
    trigger.focus();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      open = false;
      trigger.focus();
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    if (!open) {
      void showOptions();
      return;
    }
    const buttons = [...root.querySelectorAll<HTMLButtonElement>('[role="option"]')];
    if (buttons.length === 0) return;
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1
      : event.key === "ArrowDown" ? (current + 1) % buttons.length
      : (current - 1 + buttons.length) % buttons.length;
    buttons[next]?.focus();
  }
</script>

<svelte:window onclick={(event) => {
  if (open && !root?.contains(event.target as Node)) open = false;
}} onfocusin={(event) => {
  if (open && !root?.contains(event.target as Node)) open = false;
}} />

<div bind:this={root} class="relative flex flex-col gap-1.5 {className}">
  <span id={`${id}-label`} class="text-sm text-zinc-400 light:text-zinc-600">{label}</span>
  <button
    bind:this={trigger}
    {id}
    type="button"
    {disabled}
    aria-labelledby={`${id}-label`}
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-controls={`${id}-options`}
    onkeydown={handleKeydown}
    onclick={() => open ? (open = false) : void showOptions()}
    class="flex h-10 w-full items-center justify-between gap-3 rounded-lg border border-white/10 bg-white/5 px-3 text-left text-sm text-zinc-100 outline-none transition-colors hover:border-white/25 hover:bg-white/10 focus-visible:border-white/50 disabled:cursor-not-allowed disabled:opacity-50 light:border-zinc-900/10 light:bg-white light:text-zinc-900 light:hover:border-zinc-900/25"
  >
    <span class="truncate">{selectedLabel}</span>
    <svg class="size-4 shrink-0 text-zinc-400 transition-transform {open ? 'rotate-180' : ''}" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path d="m5 7.5 5 5 5-5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </button>
  {#if open}
    <div id={`${id}-options`} role="listbox" tabindex="-1" aria-labelledby={`${id}-label`} onkeydown={handleKeydown} style:top="{optionsTop}px" style:left="{optionsLeft}px" style:width="{optionsWidth}px" style:max-height="{optionsMaxHeight}px" class="fixed z-50 overflow-auto rounded-xl border border-white/10 bg-zinc-900 p-1 shadow-xl shadow-black/30 light:border-zinc-900/10 light:bg-white light:shadow-zinc-900/15">
      {#each options as option (option.value)}
        <button
          type="button"
          role="option"
          aria-selected={option.value === value}
          tabindex={option.value === value ? 0 : -1}
          onclick={() => choose(option.value)}
          class="flex w-full items-center justify-between gap-2 rounded-lg px-3 py-2 text-left text-sm text-zinc-200 outline-none hover:bg-white/10 focus-visible:bg-white/10 aria-selected:bg-white/10 aria-selected:text-white light:text-zinc-800 light:hover:bg-zinc-100 light:focus-visible:bg-zinc-100 light:aria-selected:bg-zinc-100 light:aria-selected:text-zinc-950"
        >
          <span class="min-w-0 break-words">{option.label}</span>
          {#if option.value === value}<span class="text-emerald-400 light:text-emerald-700" aria-hidden="true">✓</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>
