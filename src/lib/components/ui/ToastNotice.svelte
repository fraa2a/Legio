<script lang="ts">
  import { onDestroy } from "svelte";
  import { toast, dismissToast } from "../../stores/toast";
  import { fly } from "svelte/transition";
  import { reducedMotion } from "../../utils/motion";
  import { t, language } from "../../i18n";
  import Icon from "./Icon.svelte";

  let notice: HTMLDivElement | undefined = $state();
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!notice) return;
    if (hideTimer !== undefined) clearTimeout(hideTimer);

    const current = $toast;
    if (current.length > 0) {
      if (notice.matches(":popover-open")) notice.hidePopover();
      notice.showPopover();
    } else if (notice.matches(":popover-open")) {
      hideTimer = setTimeout(() => notice?.hidePopover(), 180);
    }
  });

  onDestroy(() => {
    if (hideTimer !== undefined) clearTimeout(hideTimer);
  });
</script>

<div
  bind:this={notice}
  popover="manual"
  class="legio-toast flex max-w-md flex-col gap-3 overflow-visible border-0 bg-transparent p-0"
  aria-live="polite"
>
  {#each $toast as notification (notification.id)}
    <div
      transition:fly={{ y: 16, duration: reducedMotion ? 0 : 180 }}
      role={notification.tone === "error" ? "alert" : "status"}
      class="pointer-events-auto flex items-start gap-3 rounded-xl border border-white/15 px-4 py-3 text-sm shadow-xl {notification.tone === 'error' ? 'bg-red-700 text-white' : notification.tone === 'success' ? 'bg-emerald-700 text-white' : 'bg-zinc-900 text-zinc-100 light:border-zinc-900/15 light:bg-zinc-50 light:text-zinc-900'}"
    >
      <p class="min-w-0 flex-1 break-words">{notification.message}</p>
      {#if notification.onRetry}<button type="button" class="shrink-0 rounded px-2 py-0.5 font-semibold hover:bg-white/15" onclick={() => { dismissToast(notification.id); notification.onRetry?.(); }}>{notification.retryLabel ?? t("Riprova", $language)}</button>{/if}
      <button type="button" class="shrink-0 rounded p-0.5 hover:bg-white/15 light:hover:bg-zinc-900/10" aria-label={t("Chiudi notifica", $language)} onclick={() => dismissToast(notification.id)}><Icon name="close" size="size-4" /></button>
    </div>
  {/each}
</div>
