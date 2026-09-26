<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    open = false,
    title,
    onClose,
    children,
  }: {
    open?: boolean;
    title: string;
    onClose: () => void;
    children: Snippet;
  } = $props();

  const titleId = $props.id();

  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  aria-labelledby={titleId}
  class="w-[26rem] max-w-[calc(100vw-2rem)] rounded-2xl bg-zinc-900 text-zinc-100 backdrop:bg-black/70 open:flex light:bg-zinc-50 light:text-zinc-900"
  oncancel={(event) => {
    event.preventDefault();
    onClose();
  }}
  onclick={(event) => {
    if (event.target === dialog) onClose();
  }}
>
  <div class="flex flex-col gap-5 p-6">
    <h2 id={titleId} class="text-lg font-semibold">{title}</h2>
    {@render children()}
  </div>
</dialog>
