<script lang="ts">
  import { tick } from "svelte";
  import { slide } from "svelte/transition";
  import { t, language } from "../../i18n";
  import { reducedMotion } from "../../utils/motion";
  import Icon from "../../components/ui/Icon.svelte";
  import type { ProfileMenuOption } from "./launch-profiles";

  let {
    label,
    options,
    selectedId,
    variant = "secondary",
    shape = "rect",
    size = "md",
    menuAbove = false,
    disabled = false,
    class: className = "",
    onPlay,
  }: {
    label: string;
    options: ProfileMenuOption[];
    selectedId: string | null;
    variant?: "primary" | "secondary";
    shape?: "rect" | "pill";
    size?: "md" | "hero";
    // Opens the menu over the control instead of under it, for controls at the bottom of a clipped area.
    menuAbove?: boolean;
    disabled?: boolean;
    class?: string;
    onPlay: (profileId: string | null) => void;
  } = $props();

  const menuId = $props.id();

  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let menu = $state<HTMLDivElement>();
  let open = $state(false);
  // Keeps the rounded box shape until the menu has finished collapsing.
  let collapsing = $state(false);

  const hero = $derived(size === "hero");
  const surface = $derived(open || collapsing);
  const iconSize = $derived(hero ? "xl" : "md");
  const selected = $derived(options.find((option) => option.id === selectedId) ?? options[0]);
  const rowHeight = $derived(hero ? "h-control-hero" : "h-control-md");
  const boxShape = $derived(surface ? (hero ? "rounded-card" : "rounded-control") : shape === "pill" ? "rounded-full" : hero ? "rounded-card" : "rounded-control");
  const colors = $derived(variant === "primary"
    ? "bg-legio-play text-white hover:bg-legio-play-hover disabled:hover:bg-legio-play"
    : "bg-legio-fill-hover text-legio-fg hover:bg-legio-fill-active disabled:hover:bg-legio-fill-hover");

  $effect(() => {
    if (disabled) open = false;
  });

  async function expand(): Promise<void> {
    if (disabled) return;
    open = true;
    collapsing = false;
    await tick();
    menu?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus();
  }

  function close(restoreFocus: boolean): void {
    if (!open) return;
    open = false;
    collapsing = !reducedMotion;
    if (restoreFocus) trigger.focus();
  }

  function choose(id: string | null): void {
    close(true);
    onPlay(id);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      close(true);
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    if (!open) {
      void expand();
      return;
    }
    const items = [...(menu?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]') ?? [])];
    if (items.length === 0) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
      : event.key === "ArrowDown" ? (current + 1) % items.length
      : (current - 1 + items.length) % items.length;
    items[next]?.focus();
  }
</script>

<svelte:window
  onfocusin={(event) => { if (open && !root.contains(event.target as Node)) close(false); }}
  onpointerdown={(event) => { if (open && !root.contains(event.target as Node)) close(false); }}
/>

<div bind:this={root} class="relative flex shrink-0 flex-col {menuAbove ? 'justify-end' : 'justify-start'} {rowHeight}">
  <div class="relative z-20 flex shrink-0 overflow-hidden {menuAbove ? 'flex-col-reverse' : 'flex-col'} {boxShape} {surface ? 'legio-popover shadow-xl shadow-black/30 light:shadow-zinc-900/15' : ''}">
    <div class="flex shrink-0 {rowHeight}">
      <button
        type="button"
        {disabled}
        title={t("Avvia con il profilo: {0}", $language, [selected.name])}
        onclick={() => onPlay(selectedId)}
        class="inline-flex min-w-0 flex-1 items-center justify-center gap-2 transition-colors disabled:cursor-not-allowed disabled:opacity-50 {hero ? 'px-4 text-xl font-bold' : 'px-4 text-sm font-medium'} {colors} {className}"
      >
        <Icon name="play" size={iconSize} />
        {label}
      </button>
      <button
        bind:this={trigger}
        type="button"
        {disabled}
        aria-label={t("Scegli il profilo di avvio: {0}", $language, [selected.name])}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onkeydown={handleKeydown}
        onclick={() => (open ? close(true) : void expand())}
        class="flex shrink-0 items-center justify-center border-l {variant === 'primary' ? 'border-white/25' : 'border-legio-line-subtle'} transition-colors disabled:cursor-not-allowed disabled:opacity-50 {hero ? 'w-control-hero' : 'w-control-md'} {colors}"
      >
        <span class="transition-transform motion-reduce:transition-none {open ? (menuAbove ? '' : 'rotate-180') : (menuAbove ? 'rotate-180' : '')}"><Icon name="chevron-down" size={iconSize} /></span>
      </button>
    </div>
    {#if open}
      <div
        bind:this={menu}
        id={menuId}
        role="menu"
        tabindex="-1"
        aria-label={t("Profili di avvio", $language)}
        transition:slide={{ duration: reducedMotion ? 0 : 200 }}
        onoutroend={() => (collapsing = false)}
        onkeydown={handleKeydown}
        class="flex flex-col gap-0.5 p-1 text-legio-fg {menuAbove ? 'border-t-0 border-b' : 'border-t'} border-legio-line-subtle"
      >
        {#each options as option (option.id ?? "base")}
          <button
            type="button"
            role="menuitemradio"
            aria-checked={option.id === selectedId}
            tabindex={option.id === selectedId ? 0 : -1}
            onclick={() => choose(option.id)}
            class="flex min-h-control-md w-full items-center justify-between gap-2 rounded-control px-3 py-2 text-left text-sm outline-none hover:bg-legio-fill-hover focus-visible:bg-legio-fill-hover aria-checked:bg-legio-fill-hover"
          >
            <span class="min-w-0 break-words">{option.name}</span>
            {#if option.id === selectedId}<Icon name="check" class="shrink-0 text-legio-success" />{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>
