<script lang="ts">
  import { tick, type Snippet } from "svelte";

  let { text, children }: { text: string; children: Snippet } = $props();

  const showDelay = 450;
  const gap = 8;
  const margin = 8;

  let anchor: HTMLSpanElement;
  let bubble = $state<HTMLSpanElement>();
  let visible = $state(false);
  let position = $state({ top: 0, left: 0 });
  let timer: ReturnType<typeof setTimeout> | undefined;

  function schedule(delay: number): void {
    clearTimeout(timer);
    timer = setTimeout(() => void show(), delay);
  }

  async function show(): Promise<void> {
    visible = true;
    await tick();
    if (!visible || !bubble) return;
    bubble.showPopover();
    const target = anchor.getBoundingClientRect();
    const size = bubble.getBoundingClientRect();
    const below = target.bottom + gap + size.height <= window.innerHeight - margin;
    position = {
      top: below ? target.bottom + gap : Math.max(margin, target.top - gap - size.height),
      left: Math.min(Math.max(margin, target.left + target.width / 2 - size.width / 2), window.innerWidth - size.width - margin),
    };
  }

  function hide(): void {
    clearTimeout(timer);
    visible = false;
  }
</script>

<svelte:window onkeydown={(event) => { if (event.key === "Escape") hide(); }} />

<span
  bind:this={anchor}
  role="presentation"
  class="inline-flex shrink-0"
  onpointerenter={() => schedule(showDelay)}
  onpointerleave={hide}
  onpointerdown={hide}
  onfocusin={() => schedule(0)}
  onfocusout={hide}
>
  {@render children()}
  {#if visible}
    <span
      bind:this={bubble}
      popover="manual"
      aria-hidden="true"
      style:inset="auto"
      style:top="{position.top}px"
      style:left="{position.left}px"
      class="pointer-events-none fixed m-0 max-w-64 rounded-lg border border-white/15 bg-zinc-900 px-2.5 py-1.5 text-xs font-medium text-zinc-100 shadow-lg shadow-black/30 light:border-zinc-900/15 light:bg-zinc-50 light:text-zinc-900"
    >{text}</span>
  {/if}
</span>
