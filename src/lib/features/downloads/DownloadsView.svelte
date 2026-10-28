<script lang="ts">
  import { t, language } from "../../i18n";
  import {
    isActiveDownloadStatus,
    type DownloadJob,
  } from "../../services/downloads";
  import Button from "../../components/ui/Button.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { games } from "../../stores/games";
  import { launchStateByGame, pendingGameId, playGame } from "../../stores/launch";
  import { selectSection } from "../../stores/navigation";
  import {
    bandwidthLimit,
    browseInstalledFolder,
    downloads,
    finishedDownloadCount,
    installedFolder,
    pauseJob,
    removeFinishedJobs,
    removeJob,
    reorderJobs,
    resumeJob,
    retryJob,
  } from "../../stores/downloads";
  import { toMessage } from "../../utils/errors";
  import ActiveDownloadCard from "./ActiveDownloadCard.svelte";
  import CompletedDownloads from "./CompletedDownloads.svelte";
  import DownloadList from "./DownloadList.svelte";
  import QueueShutdownPanel from "./QueueShutdownPanel.svelte";
  import SystemInfoPanel from "./SystemInfoPanel.svelte";
  import CancelDownloadDialog from "./CancelDownloadDialog.svelte";
  import {
    activeSpeed,
    isReorderable,
    reorderPayload,
    downloadBytes,
    splitDownloads,
  } from "./downloads-model";

  let actionError = $state<string | null>(null);
  let pendingJob = $state<string | null>(null);
  let cancelTarget = $state<DownloadJob | null>(null);
  let removeTarget = $state<DownloadJob | null>(null);
  let clearingFinished = $state(false);

  const sections = $derived(splitDownloads($downloads.data));
  const pending = $derived(
    [sections.primary, ...sections.processing, ...sections.queue].filter(
      (job): job is DownloadJob => job !== null,
    ),
  );
  const hasPendingWork = $derived(
    $downloads.data.some((job) => isActiveDownloadStatus(job.status) || job.status === "paused"),
  );
  const required = $derived(downloadBytes(pending));
  const networkSpeed = $derived(activeSpeed(pending));
  const queueOffset = $derived(
    sections.primary !== null && isReorderable(sections.primary) ? 1 : 0,
  );
  const gamesById = $derived(new Map($games.data.map((game) => [game.id, game])));

  async function run(action: (id: string) => Promise<void>, job: DownloadJob): Promise<void> {
    actionError = null;
    pendingJob = job.id;
    try {
      await action(job.id);
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      pendingJob = null;
    }
  }

  async function confirmRemove(): Promise<void> {
    const job = removeTarget;
    removeTarget = null;
    if (job === null) return;
    await run(removeJob, job);
  }

  async function clearFinished(): Promise<void> {
    actionError = null;
    clearingFinished = true;
    try {
      await removeFinishedJobs();
    } catch (error) {
      actionError = toMessage(error);
    } finally {
      clearingFinished = false;
    }
  }

  async function applyReorder(ids: string[]): Promise<void> {
    actionError = null;
    try {
      await reorderJobs(ids);
    } catch (error) {
      actionError = toMessage(error);
      await downloads.reload();
    }
  }

  function queueReordered(ids: string[]): void {
    const ordered = ids
      .map((id) => sections.queue.find((job) => job.id === id))
      .filter((job): job is DownloadJob => job !== undefined);
    void applyReorder(reorderPayload(sections.primary, ordered));
  }

  function play(job: DownloadJob): void {
    const game = gamesById.get(job.id);
    if (game === undefined) {
      actionError = t("{0} non è presente in libreria.", $language, [job.name]);
      return;
    }
    actionError = null;
    void playGame(game);
  }

  const canPlay = $derived((job: DownloadJob): boolean => {
    const game = gamesById.get(job.id);
    if (game === undefined) return false;
    const launch = $launchStateByGame.get(game.id);
    return $pendingGameId === null && (launch === undefined || launch.status === "idle");
  });
</script>

<div class="flex min-h-full flex-col gap-4">
  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}

  <StateBlock
    status={$downloads.status}
    hasData={$downloads.data.length > 0}
    loadingMessage={t("Caricamento dei download...", $language)}
    error={$downloads.error}
    onRetry={() => void downloads.load()}
  />

  <div class="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_17rem]">
    <div class="flex min-w-0 flex-col gap-4">
      {#if $downloads.status === "empty"}
        <section class="flex flex-col items-start justify-center gap-4 rounded-2xl legio-glass p-8">
          <h2 class="text-lg font-medium text-zinc-50 light:text-zinc-900">{t("Nessun download", $language)}</h2>
          <Button label={t("Sfoglia lo store", $language)} variant="primary" onClick={() => selectSection("store")} />
        </section>
      {/if}
        {#if sections.primary !== null}
          {@const primary = sections.primary}
          {#key primary.id}
            <ActiveDownloadCard
              job={primary}
              busy={pendingJob === primary.id}
              onPause={() => void run(pauseJob, primary)}
              onResume={() => void run(resumeJob, primary)}
              onRetry={() => void run(retryJob, primary)}
              onCancel={() => (cancelTarget = primary)}
              onRemove={() => (removeTarget = primary)}
            />
          {/key}
        {/if}

        {#if sections.processing.length > 0}
          <Panel title={t("In corso", $language)}>
            <DownloadList
              jobs={sections.processing}
              busyJob={pendingJob}
              onPause={(job) => void run(pauseJob, job)}
              onResume={(job) => void run(resumeJob, job)}
              onRetry={(job) => void run(retryJob, job)}
              onCancel={(job) => (cancelTarget = job)}
              onRemove={(job) => (removeTarget = job)}
            />
          </Panel>
        {/if}

        {#if sections.queue.length > 0}
          <Panel title={t("Coda download", $language)}>
            {#snippet actions()}
              <span class="text-xs text-zinc-500 light:text-zinc-600">
                {t("{0} in attesa", $language, [sections.queue.length])}
              </span>
            {/snippet}
            <DownloadList
              jobs={sections.queue}
              reorder
              positions
              positionOffset={queueOffset}
              busyJob={pendingJob}
              onReorder={queueReordered}
              onPause={(job) => void run(pauseJob, job)}
              onResume={(job) => void run(resumeJob, job)}
              onRetry={(job) => void run(retryJob, job)}
              onCancel={(job) => (cancelTarget = job)}
              onRemove={(job) => (removeTarget = job)}
            />
          </Panel>
        {/if}

        {#if sections.finished.length > 0}
          <Panel title={t("Completati", $language)}>
            {#snippet actions()}
              {#if $finishedDownloadCount > 0}
                <Button
                  label={t("Rimuovi completati ({0})", $language, [$finishedDownloadCount])}
                  variant="secondary"
                  size="sm"
                  disabled={clearingFinished || pendingJob !== null}
                  onClick={() => void clearFinished()}
                />
              {/if}
            {/snippet}
            <CompletedDownloads
              jobs={sections.finished}
              busyJob={pendingJob}
              {canPlay}
              onPlay={play}
              onRemove={(job) => (removeTarget = job)}
            />
          </Panel>
        {/if}
      </div>

      <div class="flex min-w-0 flex-col gap-4">
        <QueueShutdownPanel hasPendingWork={hasPendingWork} />
        <SystemInfoPanel
          folder={$installedFolder.data}
          status={$installedFolder.status}
          error={$installedFolder.error}
          downloadBytes={required}
          speedBps={networkSpeed}
          bandwidthLimit={$bandwidthLimit.data}
          onRetry={() => void installedFolder.load()}
          onOpenFolder={() => void browseInstalledFolder()}
        />
      </div>
    </div>
</div>

{#if cancelTarget !== null}
  <CancelDownloadDialog
    open
    jobId={cancelTarget.id}
    gameName={cancelTarget.name}
    onClose={() => (cancelTarget = null)}
  />
{/if}

<ConfirmDialog
  open={removeTarget !== null}
  title={t("Rimuovi dalla coda", $language)}
  message={t("Rimuovere {0} dalla coda?", $language, [removeTarget?.name ?? ""])}
  cancelLabel={t("Indietro", $language)}
  confirmLabel={t("Rimuovi", $language)}
  tone="danger"
  onConfirm={() => void confirmRemove()}
  onClose={() => (removeTarget = null)}
/>




