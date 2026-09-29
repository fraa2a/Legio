<script lang="ts">
  import { createDesktopShortcut, transferGame, type Game } from "../../services/local-state";
  import { pickGameDirectory } from "../../services/dialog";
  import { reconcileGame } from "../../stores/games";
  import TextField from "../../components/ui/TextField.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { toMessage } from "../../utils/errors";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import GameArtworkPanel from "./GameArtworkPanel.svelte";
  import GameDetailsPanel from "./GameDetailsPanel.svelte";
  import ExecutablePanel from "./ExecutablePanel.svelte";
  import GameSettingsPanel from "./GameSettingsPanel.svelte";
  import SteamAccountPanel from "./SteamAccountPanel.svelte";
  import SteamLaunchSettings from "./SteamLaunchSettings.svelte";

  let { game, onClose, onRemoved }: {
    game: Game;
    onClose: () => void;
    onRemoved: () => void;
  } = $props();

  const steamManaged = $derived(game.steamInstallPath !== null);
  const allCategories = [
    { id: "general", label: "Generali", icon: "settings" },
    { id: "locations", label: "Posizioni", icon: "folder" },
    { id: "customization", label: "Personalizzazione", icon: "image" },
    { id: "compatibility", label: "Compatibilità", icon: "wrench" },
    { id: "danger", label: "Zona pericolosa", icon: "warning" },
  ] satisfies { id: string; label: string; icon: "settings" | "folder" | "image" | "wrench" | "warning" }[];
  const categories = $derived(steamManaged
    ? allCategories.filter((category) => category.id !== "compatibility")
    : allCategories);
  let active = $state("general");
  let shortcutError = $state<string | null>(null);
  let shortcutPath = $state<string | null>(null);
  let shortcutPending = $state(false);
  const suggestedSource = $derived.by(() => {
    const path = game.executablePath ?? "";
    const separator = path.includes("\\") ? "\\" : "/";
    const marker = `${separator}installed${separator}${game.id}${separator}`;
    const markerAt = path.toLowerCase().indexOf(marker.toLowerCase());
    return markerAt < 0 ? path.replace(/[\\/][^\\/]+$/, "") : path.slice(0, markerAt + marker.length - 1);
  });
  let sourceDraft = $state<string | null>(null);
  let destinationDraft = $state("");
  let transferPending = $state(false);
  let transferError = $state<string | null>(null);
  let transferDone = $state(false);

  async function browseTransfer(which: "source" | "destination"): Promise<void> {
    try {
      const selected = await pickGameDirectory(which === "source" ? (sourceDraft ?? suggestedSource) : destinationDraft);
      if (selected !== null) {
        if (which === "source") sourceDraft = selected;
        else destinationDraft = `${selected}${selected.includes("\\") ? "\\" : "/"}${game.id}`;
      }
    } catch (error) { transferError = toMessage(error); }
  }

  async function moveGame(): Promise<void> {
    transferPending = true;
    transferError = null;
    transferDone = false;
    try {
      const result = await transferGame(game.id, sourceDraft ?? suggestedSource, destinationDraft.trim());
      reconcileGame(result.game);
      transferError = result.warning;
      sourceDraft = null;
      destinationDraft = "";
      transferDone = true;
    } catch (error) { transferError = toMessage(error); }
    finally { transferPending = false; }
  }

  async function addDesktopShortcut(): Promise<void> {
    shortcutPending = true;
    shortcutError = null;
    try { shortcutPath = await createDesktopShortcut(game.id); }
    catch (error) { shortcutError = toMessage(error); }
    finally { shortcutPending = false; }
  }

  function categoryClass(selected: boolean): string {
    return selected
      ? "bg-white/10 text-white light:bg-zinc-900/10 light:text-zinc-900"
      : "text-zinc-400 hover:bg-white/5 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/5 light:hover:text-zinc-900";
  }
</script>

<Dialog open title={game.name} size="wide" flush {onClose}>
  {#snippet actions()}
    <button type="button" aria-label="Chiudi le impostazioni del gioco" class="rounded-lg p-1.5 text-zinc-400 hover:bg-white/10 hover:text-zinc-100 light:text-zinc-500 light:hover:bg-zinc-900/10 light:hover:text-zinc-900" onclick={onClose}>
      <Icon name="close" size="h-5 w-5" />
    </button>
  {/snippet}

  <div class="flex min-h-0 flex-1">
    <nav class="flex w-52 shrink-0 flex-col gap-1 overflow-y-auto border-r border-white/15 p-2 light:border-zinc-900/15" aria-label="Sezioni delle impostazioni del gioco">
      {#each categories as category (category.id)}
        <button type="button" aria-current={active === category.id ? "page" : undefined} class="flex items-center gap-2 rounded-lg px-3 py-2 text-left text-sm font-medium transition-colors duration-200 {categoryClass(active === category.id)}" onclick={() => (active = category.id)}>
          <Icon name={category.icon} size="h-4 w-4 shrink-0" />
          {category.label}
        </button>
      {/each}
    </nav>
    <div class="min-w-0 flex-1 overflow-y-auto p-6">
      {#if active === "general"}
        <div class="flex flex-col gap-4">
          <GameDetailsPanel {game} {onRemoved} />
          <Panel title="Collegamento">
            <Button label={shortcutPending ? "Creazione..." : "Aggiungi collegamento desktop"}
              disabled={shortcutPending} onClick={() => void addDesktopShortcut()} />
            {#if shortcutPath}<p class="mt-2 break-all text-sm text-emerald-300" role="status">Collegamento creato: {shortcutPath}</p>{/if}
            {#if shortcutError}<ErrorBanner message={shortcutError} />{/if}
          </Panel>
          {#if steamManaged}
            {#key game.id}<SteamAccountPanel {game} />{/key}
            {#key `${game.id}:${shortcutPath ?? ""}`}<SteamLaunchSettings {game} />{/key}
          {:else if game.executablePath !== null}
            {#key game.id}<GameSettingsPanel {game} section="launch" />{/key}
          {:else}
            <Panel title="Launch Options">
              <p class="text-sm text-zinc-400 light:text-zinc-600">Seleziona prima un eseguibile nella sezione Posizioni.</p>
            </Panel>
          {/if}
        </div>
      {:else if active === "locations"}
        <div class="flex flex-col gap-4">
          {#if !steamManaged}<ExecutablePanel {game} />{:else}
            <Panel title="Installazione Steam">
              <p class="text-sm text-zinc-400 light:text-zinc-600">Steam gestisce il percorso di installazione per questo gioco.</p>
              <p class="break-all text-sm text-zinc-200 light:text-zinc-800">{game.steamInstallPath}</p>
            </Panel>
          {/if}
          {#if game.executablePath !== null}
            {#key game.id}<GameSettingsPanel {game} section="locations" />{/key}
          {/if}
          {#if !steamManaged && game.executablePath !== null}
            <Panel title="Trasferisci gioco">
              <div class="flex flex-col gap-3">
                <TextField id="transfer-source" label="Cartella del gioco" value={sourceDraft ?? suggestedSource}
                  oninput={(value) => (sourceDraft = value)} />
                <Button label="Sfoglia origine..." variant="secondary" disabled={transferPending}
                  onClick={() => void browseTransfer("source")} />
                <TextField id="transfer-destination" label="Nuova cartella del gioco" value={destinationDraft}
                  placeholder="Percorso completo di una cartella nuova" oninput={(value) => (destinationDraft = value)} />
                <Button label="Sfoglia destinazione..." variant="secondary" disabled={transferPending}
                  onClick={() => void browseTransfer("destination")} />
                <Button label={transferPending ? "Trasferimento..." : "Trasferisci gioco"}
                  disabled={transferPending || destinationDraft.trim().length === 0} onClick={() => void moveGame()} />
                {#if transferDone}<p class="text-sm text-emerald-300" role="status">Gioco trasferito.</p>{/if}
                {#if transferError}<ErrorBanner message={transferError} />{/if}
              </div>
            </Panel>
          {/if}
        </div>
      {:else if active === "customization"}
        <GameArtworkPanel {game} />
      {:else if active === "compatibility"}
        {#if game.executablePath !== null}
          {#key game.id}<GameSettingsPanel {game} section="compatibility" />{/key}
        {:else}
          <Panel title="Compatibilità">
            <p class="text-sm text-zinc-400 light:text-zinc-600">Seleziona prima un eseguibile per configurare il runner e gli argomenti di avvio.</p>
          </Panel>
        {/if}
      {:else}
        <GameDetailsPanel {game} {onRemoved} section="danger" />
      {/if}
    </div>
  </div>
</Dialog>
