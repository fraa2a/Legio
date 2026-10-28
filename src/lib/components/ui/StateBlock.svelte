<script lang="ts">
  import { t, language } from "../../i18n";
  import type { LoadStatus } from "../../stores/resource";
  import EmptyState from "./EmptyState.svelte";
  import ErrorBanner from "./ErrorBanner.svelte";
  import type { IconName } from "./Icon.svelte";
  import Skeleton from "./Skeleton.svelte";
  import Spinner from "./Spinner.svelte";

  let {
    status,
    hasData = false,
    loadingMessage = t("Caricamento...", $language),
    skeletonRows = 0,
    skeletonClass = "h-24",
    emptyMessage,
    emptyIcon,
    error,
    onRetry,
  }: {
    status: LoadStatus;
    hasData?: boolean;
    loadingMessage?: string;
    skeletonRows?: number;
    skeletonClass?: string;
    emptyMessage?: string;
    emptyIcon?: IconName;
    error: string | null;
    onRetry?: () => void;
  } = $props();
</script>

{#if status === "loading" && !hasData && skeletonRows > 0}
  <div class="flex flex-col gap-2" role="status">
    <span class="sr-only">{loadingMessage}</span>
    {#each Array.from({ length: skeletonRows }, (_, row) => row) as row (row)}<Skeleton class={skeletonClass} />{/each}
  </div>
{:else if status === "loading" && !hasData}
  <div
    class="flex items-center gap-3 rounded-xl bg-white/5 p-6 text-zinc-300 light:bg-zinc-100"
    role="status"
  >
    <Spinner />
    {loadingMessage}
  </div>
{:else if status === "empty" && emptyMessage}
  <EmptyState icon={emptyIcon} description={emptyMessage} />
{:else if status === "error" && error !== null}
  <ErrorBanner message={error} {onRetry} />
{/if}
