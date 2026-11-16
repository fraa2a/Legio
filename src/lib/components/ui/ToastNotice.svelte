<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { toast, dismissToast } from "../../stores/toast";
  import { fly } from "svelte/transition";
  import { reducedMotion } from "../../utils/motion";
  import { t, language } from "../../i18n";
  import type { ToastTone } from "../../stores/toast";
  import Button from "./Button.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import CopyButton from "./CopyButton.svelte";
  import { failureReport, technicalDetail } from "../../utils/error-message";

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
      hideTimer = setTimeout(() => notice?.hidePopover(), 200);
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
    {@const technical = technicalDetail(notification.message, notification.detail)}
    <div
      transition:fly={{ y: 16, duration: reducedMotion ? 0 : 200 }}
      role={notification.tone === "error" ? "alert" : "status"}
      class="legio-popover pointer-events-auto flex items-start gap-3 rounded-card p-4 text-sm text-legio-fg shadow-xl shadow-black/30 inset-ring light:shadow-zinc-900/15 {tone.line}"
    >
      <span class="mt-0.5 shrink-0 {tone.text}"><Icon name={tone.icon} /></span>
      <div class="min-w-0 flex-1 select-text break-words" data-selectable>
        <p>{notification.message}</p>
        {#if technical !== null}
          <details class="group mt-1">
            <summary class="flex w-fit cursor-pointer list-none items-center gap-1 text-xs text-legio-fg-muted transition-colors hover:text-legio-fg">
              {t("Dettagli tecnici", $language)}
              <span class="transition-transform group-open:rotate-180 motion-reduce:transition-none"><Icon name="chevron-down" size="sm" /></span>
            </summary>
            <pre class="mt-1 whitespace-pre-wrap break-words font-mono text-xs text-legio-fg-muted">{technical}</pre>
          </details>
        {/if}
      </div>
      {#if technical !== null}<CopyButton text={failureReport(notification.message, notification.detail)} size="inline-sm" label={t("Copia l'errore", $language)} />{/if}
      {#if notification.onRetry}<Button label={notification.retryLabel ?? t("Riprova", $language)} variant="link" class="shrink-0" onClick={() => { dismissToast(notification.id); notification.onRetry?.(); }} />{/if}
      <IconButton size="inline-sm" variant="plain" label={t("Chiudi notifica", $language)} onClick={() => dismissToast(notification.id)}><Icon name="close" /></IconButton>
    </div>
  {/each}
</div>
