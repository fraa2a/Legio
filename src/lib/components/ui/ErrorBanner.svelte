<script lang="ts">
  let {
    message,
    onRetry,
    retryLabel = "Riprova",
  }: {
    message: string;
    onRetry?: () => void;
    retryLabel?: string;
  } = $props();
</script>

<div
  class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-200 light:text-red-800"
  role="alert"
>
  <p class="min-w-0 break-words">{message}</p>
  {#if onRetry}
    <button
      type="button"
      class="shrink-0 rounded-lg bg-white/10 px-3 py-1.5 text-sm font-medium text-white transition-colors duration-200 hover:bg-white/20 light:bg-zinc-200 light:text-zinc-900"
      onclick={onRetry}
    >
      {retryLabel}
    </button>
  {/if}
</div>
