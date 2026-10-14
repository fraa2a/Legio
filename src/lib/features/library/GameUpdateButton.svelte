<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { gameUpdates } from "../../stores/game-updates";
  import { downloads } from "../../stores/downloads";
  import { launchStateByGame } from "../../stores/launch";
  import { queueGameUpdate } from "../../services/game-updates";
  import { selectSection } from "../../stores/navigation";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { describeDownloadStatus } from "../../services/downloads";

  let { gameId }: { gameId: string } = $props();
  let pending = $state(false);
  let confirming = $state(false);
  let error = $state<string | null>(null);
  const update = $derived($gameUpdates.data.find(update => update.gameId === gameId));
  const job = $derived($downloads.data.find(job => job.targetGameId === gameId && job.status !== "installed" && job.status !== "cancelled"));
  const running = $derived(($launchStateByGame.get(gameId)?.status ?? "idle") !== "idle");
  const percent = $derived(job === undefined || job.sizeBytes === 0 ? 0 : Math.min(100, job.downloadedBytes / job.sizeBytes * 100));
  onMount(() => { void gameUpdates.load(); });
  async function queue(accepted = false): Promise<void> {
    pending = true; error = null; confirming = false;
    try { await queueGameUpdate(gameId, accepted); await downloads.load(); }
    catch (reason) { error = toMessage(reason); }
    finally { pending = false; }
  }
</script>

{#if job !== undefined}
  <button type="button" class="relative flex h-[60px] w-[240px] shrink-0 items-center justify-center gap-2 overflow-hidden rounded-xl bg-legio-download px-4 text-xl font-bold text-white hover:bg-legio-download-hover" onclick={() => selectSection("downloads")}>
    <Icon name="download" size="h-6 w-6" />
    {job.pauseReason === "game_running" ? t("GIOCO APERTO", $language) : describeDownloadStatus(job.status).toUpperCase()}
    <span class="absolute inset-x-0 bottom-0 h-2 bg-white/30" role="progressbar" aria-label={t("Avanzamento aggiornamento", $language)} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(percent)}><span class="block h-full bg-white" style:width={`${percent}%`}></span></span>
  </button>
{:else if update !== undefined}
  <Button label={t("AGGIORNA", $language)} variant="download" class="h-[60px] w-[240px] shrink-0 rounded-xl px-4 text-xl font-bold" disabled={pending || running}
    onClick={() => { if (!update.verified) confirming = true; else void queue(); }}>
    <Icon name="download" size="h-6 w-6" />
  </Button>
{/if}
{#if error !== null}<ErrorBanner message={error} />{/if}
<Dialog open={confirming} title={t("Aggiornamento non verificato", $language)} onClose={() => (confirming = false)}>
  {#snippet children(dismiss)}
    <p class="text-sm text-zinc-300 light:text-zinc-700">{update?.name}: {update?.installedVersion} → {update?.version}. {t("Confermi il download e l'installazione dalla source non verificata?", $language)}</p>
    <div class="flex justify-end gap-2"><Button label={t("Annulla", $language)} variant="secondary" onClick={dismiss} /><Button label={t("Aggiorna comunque", $language)} variant="danger" disabled={pending || running} onClick={() => void queue(true)} /></div>
  {/snippet}
</Dialog>
