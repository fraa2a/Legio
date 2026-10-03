<script lang="ts">
  import type { LoadStatus } from "../../stores/resource";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";
  import { formatBytes } from "../../utils/format";

  let {
    folder,
    status,
    error,
    requiredBytes,
    speedBps,
    bandwidthLimit,
    onRetry,
    onOpenFolder,
  }: {
    folder: { directory: string; freeBytes: number; totalBytes: number } | null;
    status: LoadStatus;
    error: string | null;
    requiredBytes: number;
    speedBps: number;
    bandwidthLimit: number;
    onRetry: () => void;
    onOpenFolder: () => void;
  } = $props();

  const usedPercent = $derived.by(() => {
    if (folder === null || folder.totalBytes <= 0) return 0;
    return Math.min(100, ((folder.totalBytes - folder.freeBytes) / folder.totalBytes) * 100);
  });
  const insufficient = $derived(folder !== null && requiredBytes > folder.freeBytes);
</script>

<Panel title="Sistema">
  <div class="flex flex-col gap-4">
    {#if status === "idle" || status === "loading"}
      <div class="flex items-center gap-2.5 text-sm text-zinc-400 light:text-zinc-600" role="status">
        <span
          class="size-3.5 shrink-0 animate-spin rounded-full border-2 border-zinc-400 border-t-transparent"
          aria-hidden="true"
        ></span>
        Lettura dello spazio su disco...
      </div>
    {:else if status === "error" && error !== null}
      <ErrorBanner message={error} onRetry={onRetry} />
    {/if}

    {#if folder !== null}
      <div class="flex flex-col gap-1.5">
        <p class="text-xs text-zinc-400 light:text-zinc-600">Spazio disponibile</p>
        <p class="text-sm font-medium tabular-nums text-zinc-50 light:text-zinc-900">
          {formatBytes(folder.freeBytes)}
          <span class="font-normal text-zinc-500">su {formatBytes(folder.totalBytes)}</span>
        </p>
        <ProgressBar
          value={usedPercent}
          tone={insufficient ? "danger" : "accent"}
          label="Spazio occupato sul disco di installazione"
          class="h-1.5"
        />
      </div>
    {/if}

    <div class="flex flex-col gap-1.5">
      <p class="text-xs text-zinc-400 light:text-zinc-600">Spazio richiesto</p>
      <p
        class="text-sm font-medium tabular-nums {insufficient ? 'text-red-300 light:text-red-700' : 'text-zinc-50 light:text-zinc-900'}"
      >
        {formatBytes(requiredBytes)}
        {#if insufficient}
          <span class="font-normal">· spazio insufficiente</span>
        {/if}
      </p>
    </div>

    {#if folder !== null}
      <div class="flex flex-col gap-1.5">
        <p class="text-xs text-zinc-400 light:text-zinc-600">Directory di installazione</p>
        <p class="break-all text-xs text-zinc-300 light:text-zinc-700">{folder.directory}</p>
        <Button label="Apri la cartella" variant="secondary" class="mt-1 self-start" onClick={onOpenFolder} />
      </div>
    {/if}

    <div class="flex flex-col gap-1.5 border-t border-white/10 pt-4 light:border-zinc-900/10">
      <p class="text-xs text-zinc-400 light:text-zinc-600">Utilizzo della rete</p>
      <p class="text-sm font-medium tabular-nums text-zinc-50 light:text-zinc-900">
        {formatBytes(speedBps)}/s
      </p>
      <p class="text-xs text-zinc-500 light:text-zinc-500">
        {bandwidthLimit > 0 ? `Limite impostato a ${formatBytes(bandwidthLimit)}/s` : "Nessun limite di banda"}
      </p>
    </div>
  </div>
</Panel>
