<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    open = false,
    title,
    size = "default",
    flush = false,
    onClose,
    actions,
    children,
  }: {
    open?: boolean;
    title: string;
    size?: "default" | "wide";
    flush?: boolean;
    onClose: () => void;
    actions?: Snippet;
    children: Snippet;
  } = $props();

  const titleId = $props.id();

  let dialog: HTMLDialogElement | undefined = $state();

  const widthClass = $derived(size === "wide" ? "w-[52rem]" : "w-[26rem]");

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.preventDefault();
    onClose();
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby={titleId}
  class="m-auto {widthClass} max-w-[calc(100vw-2rem)] rounded-2xl border border-white/15 bg-zinc-900 text-zinc-100 shadow-2xl shadow-black/40 backdrop:bg-black/70 light:border-zinc-900/15 light:bg-zinc-50 light:text-zinc-900 {flush
    ? 'h-[min(40rem,calc(100dvh-3rem))] overflow-hidden'
    : 'max-h-[calc(100dvh-3rem)] overflow-y-auto'}"
  oncancel={(event) => {
    event.preventDefault();
    onClose();
  }}
  onkeydown={handleKeydown}
  onclick={(event) => {
    if (event.target === dialog) onClose();
  }}
>
  <div class={flush ? 'flex h-full min-h-0 flex-col' : 'flex flex-col gap-5 p-6'}>
    <div
      class="flex items-center justify-between gap-4 {flush
        ? 'shrink-0 border-b border-white/15 px-6 py-4 light:border-zinc-900/15'
        : ''}"
    >
      <h2 id={titleId} class="text-lg font-semibold">{title}</h2>
      {#if actions}
        <div class="shrink-0">{@render actions()}</div>
      {/if}
    </div>
    {@render children()}
  </div>
</dialog>
