<script lang="ts">
  import type { Snippet } from "svelte";
  import { t, language } from "../../i18n";
  import Icon, { type IconName } from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";

  type Tone = "info" | "success" | "warning" | "danger";

  let {
    tone = "info",
    title,
    message,
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
</script>

{#snippet list()}
  <ul aria-label={title} class="mt-1 list-disc space-y-0.5 pl-5">
    {#each items as item, index (index)}<li>{item}</li>{/each}
  </ul>
{/snippet}

<div
  role={tone === "danger" ? "alert" : "status"}
  class="flex items-start gap-3 {banner ? `rounded-xl border px-4 py-3 text-sm ${style.box}` : 'text-xs'} {className}"
>
  <span class="mt-0.5 shrink-0 {style.text}"><Icon name={style.icon} size={banner ? "size-4" : "size-3.5"} /></span>
  <div class="min-w-0 flex-1 break-words text-zinc-300 light:text-zinc-700">
    {#if title}<p class="font-medium text-zinc-50 light:text-zinc-900">{title}</p>{/if}
    {#if message}<p>{message}</p>{/if}
    {@render children?.()}
    {#if items.length > collapseAfter}
      <details class="group mt-1">
        <summary class="flex w-fit cursor-pointer list-none items-center gap-1 text-xs text-zinc-400 transition-colors hover:text-zinc-100 light:text-zinc-600 light:hover:text-zinc-900">
          {t("Mostra i dettagli ({0})", $language, [items.length])}
          <span class="transition-transform group-open:rotate-180 motion-reduce:transition-none"><Icon name="chevron-down" size="size-3.5" /></span>
        </summary>
        {@render list()}
      </details>
    {:else if items.length > 0}
      {@render list()}
    {/if}
  </div>
  {#if onRetry}<button type="button" class="shrink-0 rounded px-2 py-0.5 font-semibold text-zinc-100 hover:bg-white/10 light:text-zinc-900 light:hover:bg-zinc-900/10" onclick={onRetry}>{retryLabel}</button>{/if}
  {#if onDismiss}<IconButton size="xs" variant="plain" label={t("Chiudi avviso", $language)} onClick={onDismiss}><Icon name="close" size="size-3.5" /></IconButton>{/if}
</div>
