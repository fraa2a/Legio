<script lang="ts">
  import { onDestroy, type Snippet } from "svelte";
  import { reducedMotion } from "../../utils/motion";

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
    size?: "default" | "wide" | "large";
    flush?: boolean;
    onClose: () => void;
    actions?: Snippet<[close: () => void]>;
    children: Snippet<[close: () => void]>;
  } = $props();

  const titleId = $props.id();

  let dialog: HTMLDialogElement | undefined = $state();

  const widthClass = $derived(size === "large" ? "w-[72rem]" : size === "wide" ? "w-[52rem]" : "w-[26rem]");
  const dialogDuration = reducedMotion ? 0 : 180;
  let openFrame = 0;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!dialog) return;
    cancelAnimationFrame(openFrame);
    if (open) {
      if (closeTimer !== undefined) {
        clearTimeout(closeTimer);
        closeTimer = undefined;
      }
      if (!dialog.open) dialog.showModal();
      dialog.dataset.visible = "false";
      openFrame = requestAnimationFrame(() => {
        openFrame = requestAnimationFrame(() => {
          if (!open || !dialog?.open) return;
          dialog.dataset.visible = "true";
        });
      });
    } else if (dialog.open) close(false);
    return () => cancelAnimationFrame(openFrame);
  });

  function close(notifyParent = true): void {
    if (!dialog?.open || closeTimer !== undefined) return;
    cancelAnimationFrame(openFrame);
    dialog.dataset.visible = "false";
    closeTimer = setTimeout(() => {
      dialog?.close();
      closeTimer = undefined;
      if (notifyParent) onClose();
    }, dialogDuration);
  }

  onDestroy(() => {
    cancelAnimationFrame(openFrame);
    if (closeTimer !== undefined) clearTimeout(closeTimer);
  });

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.preventDefault();
    close();
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby={titleId}
  data-visible="false"
  class="legio-dialog m-auto {widthClass} max-w-[calc(100vw-2rem)] rounded-2xl border border-white/15 bg-zinc-900 text-zinc-100 shadow-2xl shadow-black/40 light:border-zinc-900/15 light:bg-zinc-50 light:text-zinc-900 {flush
    ? size === 'large' ? 'h-[min(48rem,calc(100dvh-3rem))] overflow-hidden' : 'h-[min(40rem,calc(100dvh-3rem))] overflow-hidden'
    : 'max-h-[calc(100dvh-3rem)] overflow-y-auto'}"
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  onkeydown={handleKeydown}
  onclick={(event) => {
    if (event.target === dialog) close();
  }}
>
  <div
    class={flush ? 'relative z-10 flex h-full min-h-0 flex-col' : 'relative z-10 flex flex-col gap-5 p-6'}
  >
    <div
      class="flex items-center justify-between gap-4 {flush
        ? 'shrink-0 border-b border-white/15 px-6 py-4 light:border-zinc-900/15'
        : ''}"
    >
      <h2 id={titleId} class="text-lg font-semibold">{title}</h2>
      {#if actions}
        <div class="shrink-0">{@render actions(() => close())}</div>
      {/if}
    </div>
    {@render children(() => close())}
  </div>
</dialog>
