<script lang="ts">
  import type { LoadStatus } from "../../stores/resource";
  import ErrorBanner from "./ErrorBanner.svelte";

  let {
    status,
    hasData = false,
    loadingMessage = "Caricamento...",
    emptyMessage,
    error,
    onRetry,
  }: {
    status: LoadStatus;
    hasData?: boolean;
    loadingMessage?: string;
    emptyMessage?: string;
    error: string | null;
    onRetry?: () => void;
  } = $props();
</script>

{#if status === "loading" && !hasData}
  <div
    class="flex items-center gap-3 rounded-xl bg-white/5 p-6 text-zinc-300 light:bg-zinc-100"
    role="status"
  >
    <span
      class="size-4 shrink-0 animate-spin rounded-full border-2 border-zinc-400 border-t-transparent"
      aria-hidden="true"
    ></span>
    {loadingMessage}
  </div>
{:else if status === "empty" && emptyMessage}
  <p class="rounded-xl bg-white/5 p-6 text-zinc-400 light:bg-zinc-100 light:text-zinc-600">
    {emptyMessage}
  </p>
{:else if status === "error" && error !== null}
  <ErrorBanner message={error} {onRetry} />
{/if}
