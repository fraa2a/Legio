<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import type { Game } from "../../services/local-state";
  import { getGameStartSteam, setGameStartSteam } from "../../services/game-settings";
  import { toMessage } from "../../utils/errors";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import ToggleSetting from "../../components/ui/ToggleSetting.svelte";

  let { game }: { game: Game } = $props();

  let enabled = $state(true);
  let loading = $state(true);
  let pending = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    void getGameStartSteam(game.id)
      .then((value) => { enabled = value; })
      .catch((cause) => { error = toMessage(cause); })
      .finally(() => { loading = false; });
  });

  async function change(value: boolean): Promise<void> {
    const previous = enabled;
    enabled = value;
    error = null;
    pending = true;
    try {
      await setGameStartSteam(game.id, value);
    } catch (cause) {
      enabled = previous;
      error = toMessage(cause);
    } finally {
      pending = false;
    }
  }
</script>

<Panel title={t("Steam", $language)}>
  <ToggleSetting label={t("Apri Steam prima del gioco", $language)} checked={enabled} defaultValue={true} disabled={loading || pending} onChange={(value) => void change(value)} />
  <p class="text-sm text-legio-fg-muted">{t("Legio avvia Steam e attende l'accesso all'account prima di aprire il gioco.", $language)}</p>
  {#if error !== null}<ErrorBanner message={error} />{/if}
</Panel>
