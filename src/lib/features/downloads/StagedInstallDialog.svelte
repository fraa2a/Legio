<script lang="ts">
  import { t, language } from "../../i18n";
  import { canFinalizeDownload } from "../../services/downloads";
  import Badge from "../../components/ui/Badge.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import {
    chooseStagedCandidate,
    finalizeStagedInstall,
    resetStagedInstall,
    rescanStagedCandidates,
    stagedInstall,
  } from "../../stores/staged-install";

  let { gameName, status }: { gameName: string; status: string } = $props();

  const group = $props.id();

  const signalLabels: Record<string, string> = $derived({
    game_name_match: t("nome del gioco", $language),
    game_root: t("cartella radice", $language),
  });

  const canConfirm = $derived(
    $stagedInstall.status === "ready" &&
      $stagedInstall.selectedPath !== null &&
      canFinalizeDownload(status),
  );

  function describeSignals(signals: string[]): string {
    return signals.map((signal) => signalLabels[signal] ?? signal).join(", ");
  }

  async function confirm(): Promise<void> {
    if (await finalizeStagedInstall()) resetStagedInstall();
  }
</script>

<Dialog
  open={$stagedInstall.jobId !== null}
  title="{t("Installa ", $language)}{gameName}"
  size="wide"
  onClose={resetStagedInstall}
>
  {#snippet children(dismiss)}
  {#if $stagedInstall.status === "loading" || $stagedInstall.finalizing}
    <p role="status">
      <span
        class="size-4 shrink-0 animate-spin rounded-full border-2 border-zinc-400 border-t-transparent"
        aria-hidden="true"
      ></span>
    </p>
  {/if}

  {#if $stagedInstall.status === "error" && $stagedInstall.error !== null}
    <ErrorBanner
      message={$stagedInstall.error}
      onRetry={() => void rescanStagedCandidates()}
    />
  {/if}

  {#if $stagedInstall.status === "empty"}
    <p class="text-sm text-zinc-400 light:text-zinc-600">{t("\n      Nessun eseguibile trovato nei file estratti.\n    ", $language)}</p>
  {/if}

  {#if $stagedInstall.candidates.length > 0}
    <fieldset class="flex flex-col gap-2" disabled={$stagedInstall.finalizing}>
      <legend class="sr-only">{t("Eseguibili trovati", $language)}</legend>
      {#each $stagedInstall.candidates as candidate (candidate.relativePath)}
        <label
          class="flex cursor-pointer items-start gap-3 rounded-lg bg-white/5 p-3 has-checked:bg-white/10 light:bg-zinc-100"
        >
          <input
            type="radio"
            name={group}
            class="mt-1"
            checked={$stagedInstall.selectedPath === candidate.relativePath}
            onchange={() => chooseStagedCandidate(candidate.relativePath)}
          />
          <span class="min-w-0 flex-1">
            <span class="block break-all text-sm text-zinc-100 light:text-zinc-900">
              {candidate.relativePath}
            </span>
            <span class="mt-1 flex flex-wrap items-center gap-2 text-xs text-zinc-500">
              <Badge tone="neutral" title={t("Punteggio {0}", $language, [candidate.score])} />
              {#if candidate.signals.length > 0}
                <span>{describeSignals(candidate.signals)}</span>
              {/if}
            </span>
          </span>
        </label>
      {/each}
    </fieldset>
  {/if}

  {#if $stagedInstall.error !== null && $stagedInstall.status !== "error"}
    <ErrorBanner message={$stagedInstall.error} />
  {/if}

  <div class="flex justify-end gap-2">
    <Button
      label={t("Annulla", $language)}
      variant="secondary"
      disabled={$stagedInstall.finalizing}
      onClick={dismiss}
    />
    <Button
      label={t("Installa", $language)}
      disabled={!canConfirm || $stagedInstall.finalizing}
      onClick={() => void confirm()}
    />
  </div>
  {/snippet}
</Dialog>
