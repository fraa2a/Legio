<script lang="ts">
  import { tick } from "svelte";
  import { get } from "svelte/store";
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import CopyButton from "../../components/ui/CopyButton.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import { noticeTones } from "../../components/ui/notice-tones";
  import {
    clearNotifications,
    markNotificationsRead,
    notifications,
    readNotificationsUpTo,
    retryNotification,
    unreadNotifications,
  } from "../../stores/notifications";
  import { failureReport, technicalDetail } from "../../utils/error-message";
  import { formatDateTime, formatRelativeTime } from "../../utils/format";
  import { reducedMotion } from "../../utils/motion";

  const panelId = $props.id();
  const duration = reducedMotion ? 0 : 200;
  // Matches --legio-ease in app.css.
  const easing = "cubic-bezier(0.2, 0, 0, 1)";
  const collapsed = { height: "0px" };
  const openSurface = "legio-popover shadow-xl shadow-black/30 light:shadow-zinc-900/15";

  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let panel = $state<HTMLDivElement>();
  let open = $state(false);
  let closing = $state(false);
  let panelTop = $state(0);
  let panelRight = $state(0);
  // Entries newer than this were unread when the panel opened.
  let seenUpTo = $state(0);
  let now = $state(Date.now());

  const expanded = $derived(open && !closing);
  const unread = $derived($unreadNotifications.attention + $unreadNotifications.other);
  const triggerLabel = $derived.by(() => {
    if (unread === 0) return t("Notifiche", $language);
    if ($unreadNotifications.attention === 0) return t("Notifiche, {0} non lette", $language, [unread]);
    return t("Notifiche, {0} non lette, di cui {1} errori o avvisi", $language, [unread, $unreadNotifications.attention]);
  });

  $effect(() => {
    if (!open) return;
    const timer = setInterval(() => { now = Date.now(); }, 30000);
    window.addEventListener("resize", dismiss);
    return () => {
      clearInterval(timer);
      window.removeEventListener("resize", dismiss);
    };
  });

  async function show(): Promise<void> {
    if (open) return;
    seenUpTo = get(readNotificationsUpTo);
    now = Date.now();
    open = true;
    markNotificationsRead();
    await tick();
    if (!open || !panel) return;
    const anchor = root.getBoundingClientRect();
    panelTop = anchor.bottom;
    panelRight = window.innerWidth - anchor.right;
    panel.showPopover();
    await tick();
    if (!open || !panel?.isConnected) return;
    if (duration > 0) panel.animate([collapsed, { height: `${panel.offsetHeight}px` }], { duration, easing });
    panel.focus({ preventScroll: true });
  }

  // Collapses the panel back into the bell before removing it.
  function close(restoreFocus: boolean): void {
    if (!open || closing) return;
    if (restoreFocus) trigger.focus();
    if (duration === 0 || !panel) {
      open = false;
      return;
    }
    closing = true;
    const animation = panel.animate([{ height: `${panel.offsetHeight}px` }, collapsed], { duration, easing, fill: "forwards" });
    animation.onfinish = dismiss;
  }

  // Closes at once when the window resizes and the panel would no longer line up.
  function dismiss(): void {
    open = false;
    closing = false;
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || !expanded) return;
    event.preventDefault();
    event.stopPropagation();
    close(true);
  }
</script>

<svelte:window onclick={(event) => {
  if (open && !root?.contains(event.target as Node)) close(false);
}} onfocusin={(event) => {
  if (open && !root?.contains(event.target as Node)) close(false);
}} />

<!-- The bell box and the panel form one surface: the box becomes the tab the panel hangs from. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={root}
  onkeydown={handleKeydown}
  class="relative flex h-control-md shrink-0 items-center p-1 {open ? `rounded-t-card ${openSurface}` : 'legio-glass rounded-card'}"
>
  <button
    bind:this={trigger}
    type="button"
    aria-label={triggerLabel}
    aria-haspopup="dialog"
    aria-expanded={expanded}
    aria-controls={panelId}
    onclick={() => expanded ? close(true) : void show()}
    class="relative inline-flex size-control-sm items-center justify-center rounded-control text-legio-fg-muted transition-colors hover:bg-legio-fill-hover hover:text-legio-fg {expanded ? 'bg-legio-fill-hover text-legio-fg' : ''}"
  >
    <Icon name="bell" />
    {#if unread > 0}
      <span
        aria-hidden="true"
        class="absolute -top-1 -right-1 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-xs leading-none font-semibold {$unreadNotifications.attention > 0 ? 'bg-legio-error text-white' : 'bg-legio-accent text-legio-accent-text'}"
      >
        {unread > 99 ? "99+" : unread}
      </span>
    {/if}
  </button>
  {#if open}
    <div
      bind:this={panel}
      id={panelId}
      popover="manual"
      role="dialog"
      tabindex="-1"
      aria-labelledby="{panelId}-title"
      style:top="{panelTop}px"
      style:right="{panelRight}px"
      style:left="auto"
      class="fixed m-0 flex max-h-[70vh] w-96 max-w-[calc(100vw-1rem)] flex-col overflow-hidden rounded-b-card rounded-tl-card p-0 text-legio-fg focus:outline-none {openSurface}"
    >
      <div class="flex shrink-0 items-center justify-between gap-2 px-4 pt-3 pb-2">
        <h2 id="{panelId}-title" class="text-sm font-semibold">{t("Notifiche", $language)}</h2>
        <Button label={t("Cancella tutto", $language)} variant="ghost" size="sm" disabled={$notifications.length === 0} onClick={() => { clearNotifications(); panel?.focus(); }} />
      </div>
      {#if $notifications.length === 0}
        <p class="px-4 pt-4 pb-6 text-center text-sm text-legio-fg-subtle">{t("Nessuna notifica in questa sessione.", $language)}</p>
      {:else}
        <ol class="flex min-h-0 flex-col gap-0.5 overflow-y-auto px-2 pb-2">
          {#each $notifications as entry (entry.id)}
            {@const tone = noticeTones[entry.tone]}
            {@const technical = technicalDetail(entry.message, entry.detail)}
            <li class="flex items-start gap-3 rounded-control px-2 py-2 text-sm {entry.id > seenUpTo ? 'bg-legio-fill' : ''}">
              <span class="mt-0.5 shrink-0 {tone.text}"><Icon name={tone.icon} /></span>
              <div class="flex min-w-0 flex-1 flex-col gap-1">
                <p class="break-words select-text">
                  {#if entry.id > seenUpTo}<span class="sr-only">{t("Nuova", $language)}:</span>{/if}
                  {entry.message}
                </p>
                {#if technical !== null}
                  <details class="group">
                    <summary class="flex w-fit cursor-pointer list-none items-center gap-1 text-xs text-legio-fg-muted transition-colors hover:text-legio-fg">
                      {t("Dettagli tecnici", $language)}
                      <span class="transition-transform group-open:rotate-180 motion-reduce:transition-none"><Icon name="chevron-down" size="sm" /></span>
                    </summary>
                    <pre class="mt-1 whitespace-pre-wrap break-words font-mono text-xs text-legio-fg-muted select-text">{technical}</pre>
                  </details>
                {/if}
                <div class="flex items-center gap-3 text-xs text-legio-fg-subtle">
                  <time datetime={new Date(entry.time).toISOString()} title={formatDateTime(entry.time / 1000, $language)}>{formatRelativeTime(entry.time, now, $language)}</time>
                  {#if technical !== null}<CopyButton text={failureReport(entry.message, entry.detail)} size="inline-sm" label={t("Copia l'errore", $language)} />{/if}
                  {#if entry.retry !== null}
                    <Button label={entry.retry.label ?? t("Riprova", $language)} variant="link" size="sm" onClick={() => void retryNotification(entry.id)} />
                  {/if}
                </div>
              </div>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  {/if}
</div>
