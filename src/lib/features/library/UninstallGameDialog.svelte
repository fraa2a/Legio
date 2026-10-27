<script lang="ts">
  import { showToast } from "../../stores/toast";
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { getGameUninstallTarget, type Game } from "../../services/local-state";
  import { uninstallGame } from "../../stores/games";
  import { toMessage } from "../../utils/errors";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";

  let { game, onClose, onRemoved }: {
    game: Game;
    onClose: () => void;
    onRemoved: (game: Game) => void;
  } = $props();

  let directory = $state<string | null>(null);
  let error = $state<string | null>(null);
  let pending = $state(false);

  onMount(() => {
    let cancelled = false;
    void getGameUninstallTarget(game.id).then(
      value => { if (!cancelled) directory = value; },
      cause => { if (!cancelled) error = toMessage(cause); },
    );
    return () => { cancelled = true; };
  });

  async function confirm(): Promise<void> {
    if (pending || directory === null) return;
    pending = true;
    error = null;
    try {
      const result = await uninstallGame(game.id, directory);
      for (const warning of result?.warnings ?? []) showToast(warning, "info");
      onRemoved(game);
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      pending = false;
    }
  }
</script>

<ConfirmDialog
  title={t("Disinstalla gioco", $language)}
  message={t("Disinstallare {0}? Il gioco verrà rimosso dalla libreria e la cartella indicata verrà eliminata definitivamente, insieme a tutti i file al suo interno.", $language, [game.name])}
  confirmLabel={t("Disinstalla definitivamente", $language)}
  tone="danger"
  {pending}
  confirmDisabled={directory === null}
  {error}
  onConfirm={() => void confirm()}
  {onClose}
>
  {#if directory !== null}
    <p class="break-all rounded-lg bg-white/5 p-3 font-mono text-xs text-zinc-200 light:bg-zinc-900/5 light:text-zinc-800">{directory}</p>
    <p class="text-sm text-legio-warning">{t("Verranno eliminati anche eventuali salvataggi e mod presenti in questa cartella. L'operazione non può essere annullata.", $language)}</p>
  {:else if error === null}
    <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">{t("Verifica della cartella del gioco...", $language)}</p>
  {/if}
  {#if pending}<p class="text-sm text-zinc-400 light:text-zinc-600" role="status">{t("Disinstallazione in corso...", $language)}</p>{/if}
</ConfirmDialog>
