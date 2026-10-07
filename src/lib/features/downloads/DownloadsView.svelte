<script lang="ts">
  import { t, language } from "../../i18n";
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
  const required = $derived(downloadBytes(pending));
  const networkSpeed = $derived(activeSpeed(pending));
  const queueOffset = $derived(
    sections.primary !== null && isReorderable(sections.primary) ? 1 : 0,
  );
  const installTarget = $derived(
    $downloads.data.find((job) => job.id === $stagedInstall.jobId) ?? null,
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

  {#if $downloads.status === "empty"}
    <section class="flex flex-col items-start justify-center gap-4 rounded-2xl legio-glass p-8">
      <h2 class="text-lg font-medium text-zinc-50 light:text-zinc-900">{t("Nessun download", $language)}</h2>
      <Button label={t("Sfoglia lo store", $language)} variant="primary" onClick={() => selectSection("store")} />
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
          <Panel title={t("In corso", $language)}>
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
          <Panel title={t("Coda download", $language)}>
            {#snippet actions()}
              <span class="text-xs text-zinc-500 light:text-zinc-500">
                {sections.queue.length}{t(" in attesa\n              ", $language)}</span>
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
          <Panel title={t("Completati", $language)}>
            {#snippet actions()}
              {#if $finishedDownloadCount > 0}
                <Button
                  label="{t("Rimuovi completati (", $language)}{$finishedDownloadCount})"
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
        downloadBytes={required}
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

<Dialog open={removeTarget !== null} title={t("Rimuovi dalla coda", $language)} onClose={() => (removeTarget = null)}>
  {#snippet children(dismiss)}
  <p class="text-sm text-zinc-300 light:text-zinc-700">{t("\n    Rimuovere ", $language)}{removeTarget?.name}{t(" dalla coda?\n  ", $language)}</p>
  <div class="flex justify-end gap-2">
    <Button label={t("Indietro", $language)} variant="secondary" onClick={dismiss} />
    <Button label={t("Rimuovi", $language)} variant="danger" onClick={() => void confirmRemove()} />
  </div>
  {/snippet}
</Dialog>

{#if installTarget !== null}
  <StagedInstallDialog gameName={installTarget.name} status={installTarget.status} />
{/if}

{#if $accountSwitchGame !== null}
  <Dialog open title={t("Cambio account Steam", $language)} onClose={dismissAccountSwitch}>
  {#snippet children(dismiss)}
    <p class="text-sm text-zinc-300 light:text-zinc-700">{t("\n      Steam deve essere chiuso e riavviato per usare l'account salvato di ", $language)}{$accountSwitchGame.name}{t(". Procedere?\n    ", $language)}</p>
    <div class="flex justify-end gap-2">
      <Button label={t("Annulla", $language)} variant="secondary" onClick={dismiss} />
      <Button label={t("Riavvia e avvia", $language)} onClick={() => void confirmAccountSwitch()} />
    </div>
    {/snippet}
</Dialog>
{/if}
