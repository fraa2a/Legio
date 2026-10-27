<script lang="ts">
  import type { Snippet } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import ErrorBanner from "./ErrorBanner.svelte";

  let {
    open = true,
    title,
    message,
    confirmLabel,
    cancelLabel,
    tone = "primary",
    pending = false,
    confirmDisabled = false,
    error = null,
    onConfirm,
    onClose,
    children: details,
  }: {
    open?: boolean;
    title: string;
    message?: string;
    confirmLabel: string;
    cancelLabel?: string;
    tone?: "primary" | "danger";
    pending?: boolean;
    confirmDisabled?: boolean;
    error?: string | null;
    onConfirm: () => void;
    onClose: () => void;
    children?: Snippet;
  } = $props();
</script>

<Dialog {open} {title} dismissible={!pending} {onClose}>
  {#snippet children(dismiss)}
    {#if message}<p class="text-sm text-zinc-300 light:text-zinc-700">{message}</p>{/if}
    {@render details?.()}
    {#if error !== null}<ErrorBanner message={error} />{/if}
    <div class="flex justify-end gap-2">
      <Button label={cancelLabel ?? t("Annulla", $language)} variant="secondary" disabled={pending} onClick={dismiss} />
      <Button label={confirmLabel} variant={tone} disabled={pending || confirmDisabled} onClick={onConfirm} />
    </div>
  {/snippet}
</Dialog>
