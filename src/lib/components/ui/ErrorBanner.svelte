<script lang="ts">
  import { t, language } from "../../i18n";
  let {
    message,
    onRetry,
    retryLabel = t("Riprova", $language),
    tone = "error",
  }: {
    message: string;
    onRetry?: () => void;
    retryLabel?: string;
    tone?: "error" | "warning";
  } = $props();
</script>

<div role={tone === "error" ? "alert" : "status"}
  class="flex items-start gap-3 rounded-xl border px-4 py-3 text-sm {tone === 'error' ? 'border-red-500/30 bg-red-500/10 text-legio-error' : 'border-amber-500/30 bg-amber-500/10 text-legio-warning'}">
  <p class="min-w-0 flex-1 break-words">{message}</p>
  {#if onRetry}<button type="button" class="shrink-0 rounded px-2 py-0.5 font-semibold hover:bg-white/10 light:hover:bg-zinc-900/10" onclick={onRetry}>{retryLabel}</button>{/if}
</div>
