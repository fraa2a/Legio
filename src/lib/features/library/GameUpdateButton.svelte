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
  import DownloadProgressButton from "../downloads/DownloadProgressButton.svelte";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { describeDownloadStatus, downloadProgressPercent } from "../../services/downloads";

  let { gameId }: { gameId: string } = $props();
  let pending = $state(false);
  let confirming = $state(false);
  let error = $state<string | null>(null);
  const update = $derived($gameUpdates.data.find(update => update.gameId === gameId));
  const job = $derived($downloads.data.find(job => job.targetGameId === gameId && job.status !== "installed" && job.status !== "cancelled"));
  const running = $derived(($launchStateByGame.get(gameId)?.status ?? "idle") !== "idle");
  const percent = $derived(job === undefined ? 0 : downloadProgressPercent(job));
  onMount(() => { void gameUpdates.load(); });
  async function queue(accepted = false): Promise<void> {
    pending = true; error = null; confirming = false;
    try { await queueGameUpdate(gameId, accepted); await downloads.reload(); }
    catch (reason) { error = toMessage(reason); }
    finally { pending = false; }
  }
</script>

{#if job !== undefined}
  <DownloadProgressButton
    label={job.pauseReason === "game_running" ? t("GIOCO APERTO", $language) : describeDownloadStatus(job.status).toUpperCase()}
    value={percent}
    progressLabel={t("Avanzamento aggiornamento", $language)}
    onClick={() => selectSection("downloads")}
  />
{:else if update !== undefined}
  <Button label={t("AGGIORNA", $language)} variant="download" size="hero" class="w-60" disabled={pending || running}
    onClick={() => { if (!update.verified) confirming = true; else void queue(); }}>
    <Icon name="download" size="size-6" />
  </Button>
{/if}
{#if error !== null}<ErrorBanner message={error} />{/if}
<ConfirmDialog
  open={confirming}
  title={t("Aggiornamento non verificato", $language)}
  message="{update?.name}: {update?.installedVersion} → {update?.version}. {t("Confermi il download e l'installazione dalla source non verificata?", $language)}"
  confirmLabel={t("Aggiorna comunque", $language)}
  tone="danger"
  confirmDisabled={pending || running}
  onConfirm={() => void queue(true)}
  onClose={() => (confirming = false)}
/>
