<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { toast, dismissToast } from "../../stores/toast";
  import { fly } from "svelte/transition";
  import { reducedMotion } from "../../utils/motion";
  import { t, language } from "../../i18n";
  import type { ToastTone } from "../../stores/toast";
  import Icon, { type IconName } from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";

  const tones: Record<ToastTone, { icon: IconName; text: string; line: string }> = {
    info: { icon: "info", text: "text-legio-info", line: "inset-ring-legio-info/25" },
    success: { icon: "success", text: "text-legio-success", line: "inset-ring-legio-success/25" },
    warning: { icon: "warning", text: "text-legio-warning", line: "inset-ring-legio-warning/25" },
    error: { icon: "danger", text: "text-legio-error", line: "inset-ring-legio-error/25" },
  };

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
    {@const tone = tones[notification.tone]}
    <div
      transition:fly={{ y: 16, duration: reducedMotion ? 0 : 180 }}
      role={notification.tone === "error" ? "alert" : "status"}
      class="legio-popover pointer-events-auto flex items-start gap-3 rounded-xl px-4 py-3 text-sm text-legio-fg shadow-xl shadow-black/30 inset-ring light:shadow-zinc-900/15 {tone.line}"
    >
      <span class="mt-0.5 shrink-0 {tone.text}"><Icon name={tone.icon} size="size-4" /></span>
      <p class="min-w-0 flex-1 break-words">{notification.message}</p>
      {#if notification.onRetry}<button type="button" class="shrink-0 rounded px-2 py-0.5 font-semibold hover:bg-legio-fill-hover" onclick={() => { dismissToast(notification.id); notification.onRetry?.(); }}>{notification.retryLabel ?? t("Riprova", $language)}</button>{/if}
      <IconButton size="xs" variant="plain" label={t("Chiudi notifica", $language)} onClick={() => dismissToast(notification.id)}><Icon name="close" size="size-4" /></IconButton>
    </div>
  {/each}
</div>
