<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    open = false,
    title,
    size = "default",
    onClose,
    children,
  }: {
    open?: boolean;
    title: string;
    size?: "default" | "wide";
    onClose: () => void;
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
</script>

<dialog
  bind:this={dialog}
  aria-labelledby={titleId}
  class="{widthClass} max-w-[calc(100vw-2rem)] rounded-2xl bg-zinc-900 text-zinc-100 backdrop:bg-black/70 open:flex light:bg-zinc-50 light:text-zinc-900"
  oncancel={(event) => {
    event.preventDefault();
    onClose();
  }}
  onclick={(event) => {
    if (event.target === dialog) onClose();
  }}
>
  <div class="flex max-h-[70dvh] flex-col gap-5 overflow-y-auto p-6">
    <h2 id={titleId} class="text-lg font-semibold">{title}</h2>
    {@render children()}
  </div>
</dialog>
