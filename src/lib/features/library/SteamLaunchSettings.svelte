<script lang="ts">
  import { onMount } from "svelte";
  import type { Game } from "../../services/local-state";
  import { getSteamLaunchConfig, saveSteamLaunchConfig } from "../../services/game-settings";
  import { toMessage } from "../../utils/errors";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import { formatLaunchArguments, parseLaunchArguments } from "../../utils/launch-arguments";

  let { game }: { game: Game } = $props();
  let argumentsText = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);

  onMount(() => {
    void getSteamLaunchConfig(game.id)
      .then((config) => (argumentsText = formatLaunchArguments(config.arguments)))
      .catch((cause) => (error = toMessage(cause)))
      .finally(() => (loading = false));
  });

  async function save(): Promise<void> {
    saving = true;
    error = null;
    saved = false;
    try {
      await saveSteamLaunchConfig(game.id, {
        arguments: parseLaunchArguments(argumentsText),
      });
      saved = true;
    } catch (cause) {
      error = toMessage(cause);
    } finally {
      saving = false;
    }
  }
</script>

<Panel title="Argomenti di avvio Steam">
  {#if error !== null}<ErrorBanner message={error} />{/if}
  <TextField
    id="steam-launch-arguments"
    label="Opzioni di avvio"
    value={argumentsText}
    placeholder="-windowed -novid"
    hint="Aggiungi argomenti oppure racchiudi un valore con spazi tra virgolette."
    disabled={loading || saving}
    oninput={(value) => (argumentsText = value)}
  />
  <p class="text-xs text-zinc-500">Gli argomenti vengono passati a Steam quando avvii questo gioco da Legio.</p>
  <div class="flex items-center gap-3">
    <Button label={saving ? "Salvataggio..." : "Salva argomenti"} disabled={loading || saving} onClick={() => void save()} />
    {#if saved}<span class="text-sm text-emerald-400" role="status">Salvato</span>{/if}
  </div>
</Panel>
