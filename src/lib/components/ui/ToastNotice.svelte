<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { toast, dismissToast } from "../../stores/toast";
  import { fly } from "svelte/transition";
  import { reducedMotion } from "../../utils/motion";
  import { t, language } from "../../i18n";
  import Icon from "./Icon.svelte";

  let notice: HTMLDivElement | undefined = $state();
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let home: ParentNode | null = null;
  let sibling: ChildNode | null = null;

  function placeNotice(): void {
    if (!notice) return;
    if (home === null) { home = notice.parentNode; sibling = notice.nextSibling; }
    const dialogs = document.querySelectorAll<HTMLDialogElement>("dialog:modal");
    const target = dialogs.item(dialogs.length - 1) ?? home;
    if (target && notice.parentNode !== target) {
      if (notice.matches(":popover-open")) notice.hidePopover();
      target.appendChild(notice);
    }
  }

  $effect(() => {
    if (!notice) return;
    if (hideTimer !== undefined) clearTimeout(hideTimer);

    const current = $toast;
    placeNotice();
    if (current.length > 0) {
      if (notice.matches(":popover-open")) notice.hidePopover();
      notice.showPopover();
    } else if (notice.matches(":popover-open")) {
      hideTimer = setTimeout(() => notice?.hidePopover(), 180);
    }
  });

  onMount(() => {
    const observer = new MutationObserver(() => {
      placeNotice();
      if (get(toast).length > 0 && notice && !notice.matches(":popover-open")) notice.showPopover();
    });
    observer.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ["open"] });
    return () => {
      observer.disconnect();
      if (hideTimer !== undefined) clearTimeout(hideTimer);
      if (notice && home) home.insertBefore(notice, sibling?.parentNode === home ? sibling : null);
    };
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
