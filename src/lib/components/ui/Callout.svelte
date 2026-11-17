<script lang="ts">
  import type { Snippet } from "svelte";
  import { t, language } from "../../i18n";
  import Button from "./Button.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import Disclosure from "./Disclosure.svelte";
  import CopyButton from "./CopyButton.svelte";
  import { failureReport, technicalDetail } from "../../utils/error-message";

  type Tone = "info" | "success" | "warning" | "danger";

  let {
    tone = "info",
    title,
    message,
    detail,
    copyable = false,
    items = [],
    variant = "banner",
    onRetry,
    retryLabel = t("Riprova", $language),
    onDismiss,
    class: className = "",
    children,
  }: {
    tone?: Tone;
    title?: string;
    message?: string;
    // Technical text kept apart from the summary in `message`.
    detail?: string | null;
    copyable?: boolean;
    items?: string[];
    variant?: "banner" | "inline";
    onRetry?: () => void;
    retryLabel?: string;
    onDismiss?: () => void;
    class?: string;
    children?: Snippet;
  } = $props();

  const collapseAfter = 2;

  const styles: Record<Tone, { icon: IconName; text: string; box: string }> = {
    info: { icon: "info", text: "text-legio-info", box: "border-legio-info/25 bg-legio-info/10" },
    success: { icon: "success", text: "text-legio-success", box: "border-legio-success/25 bg-legio-success/10" },
    warning: { icon: "warning", text: "text-legio-warning", box: "border-legio-warning/25 bg-legio-warning/10" },
    danger: { icon: "danger", text: "text-legio-error", box: "border-legio-error/25 bg-legio-error/10" },
  };
  const style = $derived(styles[tone]);
  const banner = $derived(variant === "banner");
  const technical = $derived(message === undefined ? null : technicalDetail(message, detail));
  const reportText = $derived(message === undefined ? "" : failureReport(message, detail));
</script>

{#snippet list()}
  <ul aria-label={title} class="mt-1 list-disc space-y-0.5 pl-5">
    {#each items as item, index (index)}<li>{item}</li>{/each}
  </ul>
{/snippet}

<div
  role={tone === "danger" ? "alert" : "status"}
  class="flex items-start gap-3 {banner ? `rounded-card border p-inset-compact text-sm ${style.box}` : 'text-xs'} {className}"
>
  <span class="mt-0.5 shrink-0 {style.text}"><Icon name={style.icon} size={banner ? "md" : "sm"} /></span>
  <div class="min-w-0 flex-1 select-text break-words text-legio-fg" data-selectable>
    {#if title}<p class="font-medium text-legio-fg">{title}</p>{/if}
    {#if message}<p>{message}</p>{/if}
    {#if technical !== null}
      <Disclosure label={t("Dettagli tecnici", $language)}>
        <pre class="mt-1 whitespace-pre-wrap break-words font-mono text-xs text-legio-fg-muted">{technical}</pre>
      </Disclosure>
    {/if}
    {@render children?.()}
    {#if items.length > collapseAfter}
      <Disclosure label={t("Mostra i dettagli ({0})", $language, [items.length])}>
        {@render list()}
      </Disclosure>
    {:else if items.length > 0}
      {@render list()}
    {/if}
  </div>
  {#if onRetry}<Button label={retryLabel} variant="link" size={banner ? "md" : "sm"} class="shrink-0" onClick={onRetry} />{/if}
  {#if copyable && message !== undefined}<CopyButton text={reportText} size="inline-sm" label={t("Copia l'errore", $language)} />{/if}
  {#if onDismiss}<IconButton size="inline-sm" variant="plain" label={t("Chiudi avviso", $language)} onClick={onDismiss}><Icon name="close" size="sm" /></IconButton>{/if}
</div>
