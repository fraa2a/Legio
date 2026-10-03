<script lang="ts">
  import {
    type DownloadJob,
  } from "../../services/downloads";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import StateBlock from "../../components/ui/StateBlock.svelte";
  import { games } from "../../stores/games";
  import {
    accountSwitchGame,
    confirmAccountSwitch,
    dismissAccountSwitch,
    launchStateByGame,
    pendingGameId,
    playGame,
  } from "../../stores/launch";
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
  import { openStagedInstall, stagedInstall } from "../../stores/staged-install";
  import { toMessage } from "../../utils/errors";
  import ActiveDownloadCard from "./ActiveDownloadCard.svelte";
  import CompletedDownloads from "./CompletedDownloads.svelte";
  import DownloadList from "./DownloadList.svelte";
  import SystemInfoPanel from "./SystemInfoPanel.svelte";
  import CancelDownloadDialog from "./CancelDownloadDialog.svelte";
  import StagedInstallDialog from "./StagedInstallDialog.svelte";
  import {
    activeSpeed,
    isReorderable,
    reorderPayload,
    requiredBytes,
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
  const required = $derived(requiredBytes(pending));
  const networkSpeed = $derived(activeSpeed(pending));
  const queueOffset = $derived(
    sections.primary !== null && isReorderable(sections.primary) ? 1 : 0,
  );
  const installTarget = $derived(
    $downloads.data.find((job) => job.id === $stagedInstall.jobId) ?? null,
  );

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
      await downloads.load();
    }
  }

  function queueReordered(ids: string[]): void {
    const ordered = ids
      .map((id) => sections.queue.find((job) => job.id === id))
      .filter((job): job is DownloadJob => job !== undefined);
    void applyReorder(reorderPayload(sections.primary, ordered));
  }

  function play(job: DownloadJob): void {
    const game = $games.data.find((item) => item.steamAppId === job.steamAppId);
    if (game === undefined) {
      actionError = `${job.name} non è presente in libreria.`;
      return;
    }
    actionError = null;
    void playGame(game);
  }

  const canPlay = (job: DownloadJob): boolean => {
    const game = $games.data.find((item) => item.steamAppId === job.steamAppId);
    if (game === undefined) return false;
    const launch = $launchStateByGame.get(game.id);
    return $pendingGameId === null && (launch === undefined || launch.status === "idle");
  };
</script>

<div class="flex min-h-full flex-col gap-4">
  {#if actionError !== null}
    <ErrorBanner message={actionError} />
  {/if}

  <StateBlock
    status={$downloads.status}
    hasData={$downloads.data.length > 0}
    loadingMessage="Caricamento dei download..."
    error={$downloads.error}
    onRetry={() => void downloads.load()}
  />

  {#if $downloads.status === "empty"}
    <section class="flex flex-col items-start justify-center gap-4 rounded-2xl bg-white/5 p-8 light:bg-zinc-100">
      <h2 class="text-lg font-medium text-zinc-50 light:text-zinc-900">Nessun download</h2>
      <Button label="Sfoglia lo store" variant="primary" onClick={() => selectSection("store")} />
    </section>
  {/if}

  {#if $downloads.data.length > 0}
    <div class="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_17rem]">
      <div class="flex min-w-0 flex-col gap-4">
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
              onInstall={() => openStagedInstall(primary.id)}
            />
          {/key}
        {/if}

        {#if sections.processing.length > 0}
          <Panel title="In corso">
            <DownloadList
              jobs={sections.processing}
              busyJob={pendingJob}
              onPause={(job) => void run(pauseJob, job)}
              onResume={(job) => void run(resumeJob, job)}
              onRetry={(job) => void run(retryJob, job)}
              onCancel={(job) => (cancelTarget = job)}
              onRemove={(job) => (removeTarget = job)}
              onInstall={(job) => openStagedInstall(job.id)}
            />
          </Panel>
        {/if}

        {#if sections.queue.length > 0}
          <Panel title="Coda download">
            {#snippet actions()}
              <span class="text-xs text-zinc-500 light:text-zinc-500">
                {sections.queue.length} in attesa
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
              onInstall={(job) => openStagedInstall(job.id)}
            />
          </Panel>
        {/if}

        {#if sections.finished.length > 0}
          <Panel title="Completati">
            {#snippet actions()}
              {#if $finishedDownloadCount > 0}
                <Button
                  label="Rimuovi completati ({$finishedDownloadCount})"
                  variant="secondary"
                  class="h-8! px-3! text-xs!"
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

      <SystemInfoPanel
        folder={$installedFolder.data}
        status={$installedFolder.status}
        error={$installedFolder.error}
        requiredBytes={required}
        speedBps={networkSpeed}
        bandwidthLimit={$bandwidthLimit.data}
        onRetry={() => void installedFolder.load()}
        onOpenFolder={() => void browseInstalledFolder()}
      />
    </div>
  {/if}
</div>

{#if cancelTarget !== null}
  <CancelDownloadDialog
    open
    jobId={cancelTarget.id}
    gameName={cancelTarget.name}
    onClose={() => (cancelTarget = null)}
  />
{/if}

<Dialog open={removeTarget !== null} title="Rimuovi dalla coda" onClose={() => (removeTarget = null)}>
  <p class="text-sm text-zinc-300 light:text-zinc-700">
    Rimuovere {removeTarget?.name} dalla coda?
  </p>
  <div class="flex justify-end gap-2">
    <Button label="Indietro" variant="secondary" onClick={() => (removeTarget = null)} />
    <Button label="Rimuovi" variant="danger" onClick={() => void confirmRemove()} />
  </div>
</Dialog>

{#if installTarget !== null}
  <StagedInstallDialog gameName={installTarget.name} status={installTarget.status} />
{/if}

{#if $accountSwitchGame !== null}
  <Dialog open title="Cambio account Steam" onClose={dismissAccountSwitch}>
    <p class="text-sm text-zinc-300 light:text-zinc-700">
      Steam deve essere chiuso e riavviato per usare l'account salvato di {$accountSwitchGame.name}. Procedere?
    </p>
    <div class="flex justify-end gap-2">
      <Button label="Annulla" variant="secondary" onClick={dismissAccountSwitch} />
      <Button label="Riavvia e avvia" onClick={() => void confirmAccountSwitch()} />
    </div>
  </Dialog>
{/if}
