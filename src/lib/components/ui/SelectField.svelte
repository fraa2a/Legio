<script lang="ts">
  import { tick } from "svelte";
  import { placePopover } from "../../utils/popover-placement";
  import { reducedMotion } from "../../utils/motion";
  import Icon from "./Icon.svelte";

  let {
    id,
    label,
    labelHidden = false,
    value = $bindable(),
    options,
    disabled = false,
    borderless = false,
    variant = "default",
    onChange,
    class: className = "",
  }: {
    id: string;
    label: string;
    labelHidden?: boolean;
    value: string;
    options: { value: string; label: string }[];
    disabled?: boolean;
    borderless?: boolean;
    variant?: "default" | "hero" | "toolbar";
    onChange?: (value: string) => void;
    class?: string;
  } = $props();

  const duration = reducedMotion ? 0 : 200;
  // Matches --legio-ease in app.css.
  const easing = "cubic-bezier(0.2, 0, 0, 1)";
  const collapsed = { height: "0px", paddingTop: "0px", paddingBottom: "0px" };

  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let listbox = $state<HTMLDivElement>();
  let open = $state(false);
  let closing = $state(false);
  let above = $state(false);
  let optionsEdge = $state(0);
  let optionsLeft = $state(0);
  let optionsWidth = $state(0);
  let optionsMaxHeight = $state(256);
  const selectedLabel = $derived(options.find((option) => option.value === value)?.label ?? value);
  const expanded = $derived(open && !closing);

  // The list continues the visible box: the hero container, otherwise the trigger.
  const large = $derived(variant !== "default");
  const boxShape = $derived(!open
    ? (large ? "rounded-card" : "rounded-control")
    : above ? (large ? "rounded-b-card" : "rounded-b-control") : (large ? "rounded-t-card" : "rounded-t-control"));
  const listShape = $derived(above ? (large ? "rounded-t-card" : "rounded-t-control") : (large ? "rounded-b-card" : "rounded-b-control"));
  const bordered = $derived(variant === "hero" || (variant === "default" && !borderless));
  // A bordered box already draws the seam; otherwise the list draws it.
  const listBorder = $derived(bordered ? (above ? "border-x border-t" : "border-x border-b") : (above ? "border-b" : "border-t"));
  const openSurface = "legio-popover shadow-xl shadow-black/30 light:shadow-zinc-900/15";

  $effect(() => {
    if (disabled) dismiss();
  });

  $effect(() => {
    if (!open) return;
    const closeOnScroll = (event: Event) => {
      if (!(event.target instanceof Node && root.contains(event.target))) dismiss();
    };
    window.addEventListener("scroll", closeOnScroll, true);
    window.addEventListener("resize", dismiss);
    return () => {
      window.removeEventListener("scroll", closeOnScroll, true);
      window.removeEventListener("resize", dismiss);
    };
  });

  function anchor(): HTMLElement {
    return variant === "hero" ? root : trigger;
  }

  async function showOptions(): Promise<void> {
    if (disabled || open) return;
    open = true;
    await tick();
    if (!open || !listbox) return;

    const anchorRect = anchor().getBoundingClientRect();
    optionsLeft = anchorRect.left;
    optionsWidth = anchorRect.width;
    optionsMaxHeight = 256;
    await tick();
    if (!open || !listbox?.isConnected) return;
    listbox.showPopover();
    const placement = placePopover(anchorRect, listbox.scrollHeight, window.innerHeight);
    above = placement.above;
    optionsEdge = above ? window.innerHeight - anchorRect.top : anchorRect.bottom;
    optionsMaxHeight = placement.maxHeight;
    await tick();
    if (!open || !listbox?.isConnected) return;

    if (duration > 0) {
      listbox.animate([collapsed, { height: `${listbox.offsetHeight}px` }], { duration, easing });
    }
    const selected = listbox.querySelector<HTMLButtonElement>('[role="option"][aria-selected="true"]');
    (selected ?? listbox.querySelector<HTMLButtonElement>('[role="option"]'))?.focus({ preventScroll: true });
  }

  // Collapses the list back into the trigger before removing it.
  function close(restoreFocus: boolean): void {
    if (!open || closing) return;
    if (restoreFocus) trigger.focus();
    if (duration === 0 || !listbox) {
      open = false;
      return;
    }
    closing = true;
    const animation = listbox.animate([{ height: `${listbox.offsetHeight}px` }, collapsed], { duration, easing, fill: "forwards" });
    animation.onfinish = dismiss;
  }

  // Closes at once, for scrolling, resizing and disabling where the list would no longer line up.
  function dismiss(): void {
    open = false;
    closing = false;
  }

  function choose(next: string): void {
    if (onChange) onChange(next);
    else value = next;
    close(true);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && expanded) {
      event.preventDefault();
      close(true);
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
  if (open && !root?.contains(event.target as Node)) close(false);
}} onfocusin={(event) => {
  if (open && !root?.contains(event.target as Node)) close(false);
}} />

<div bind:this={root} class="relative flex flex-col {variant === 'hero' ? `h-control-hero justify-center border border-legio-line-subtle ${boxShape} ${open ? openSurface : 'legio-glass'}` : 'gap-1.5'} {className}">
  <span id={`${id}-label`} class={labelHidden ? "sr-only" : variant === "hero" ? "px-3 pt-1 text-xs text-legio-fg-muted" : "text-sm font-medium text-legio-fg"}>{label}</span>
  <button
    bind:this={trigger}
    {id}
    type="button"
    {disabled}
    aria-labelledby={`${id}-label`}
    aria-haspopup="listbox"
    aria-expanded={expanded}
    aria-controls={`${id}-options`}
    onkeydown={handleKeydown}
    onclick={() => expanded ? close(true) : void showOptions()}
    class="flex w-full items-center justify-between gap-3 px-3 text-left text-sm text-legio-fg transition-colors disabled:cursor-not-allowed disabled:opacity-50 {variant === 'hero'
      ? 'h-control-md rounded-control bg-transparent'
      : variant === 'toolbar'
        ? `h-control-md ${boxShape} ${open ? openSurface : 'legio-glass'}`
        : `h-control-md ${boxShape} ${borderless ? '' : 'border border-legio-line-control hover:border-legio-fg-muted'} ${open ? openSurface : 'legio-glass'}`}"
  >
    <span class="truncate">{selectedLabel}</span>
    <span class="shrink-0 text-legio-fg-muted transition-transform motion-reduce:transition-none {expanded ? 'rotate-180' : ''}"><Icon name="chevron-down" /></span>
  </button>
  {#if open}
    <div
      bind:this={listbox}
      id={`${id}-options`}
      popover="manual"
      role="listbox"
      tabindex="-1"
      aria-labelledby={`${id}-label`}
      onkeydown={handleKeydown}
      style:top={above ? "auto" : `${optionsEdge}px`}
      style:bottom={above ? `${optionsEdge}px` : "auto"}
      style:left="{optionsLeft}px"
      style:right="auto"
      style:width="{optionsWidth}px"
      style:max-height="{optionsMaxHeight}px"
      class="fixed z-50 m-0 overflow-auto border-legio-line-subtle p-1 {listShape} {listBorder} {openSurface}"
    >
      {#each options as option (option.value)}
        <button
          type="button"
          role="option"
          aria-selected={option.value === value}
          tabindex={option.value === value ? 0 : -1}
          onclick={() => choose(option.value)}
          class="flex w-full items-center justify-between gap-2 rounded-control px-3 py-2 text-left text-sm text-legio-fg hover:bg-legio-fill-hover focus-visible:-outline-offset-2 focus-visible:bg-legio-fill-hover aria-selected:bg-legio-fill-hover aria-selected:text-legio-fg"
        >
          <span class="min-w-0 break-words">{option.label}</span>
          {#if option.value === value}<Icon name="check" class="shrink-0 text-legio-success" />{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>
