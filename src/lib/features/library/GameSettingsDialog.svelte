<script lang="ts">
  import { t, language } from "../../i18n";
  import { createDesktopShortcut, transferGame, type Game } from "../../services/local-state";
  import { pickGameDirectory } from "../../services/dialog";
  import { reconcileGame } from "../../stores/games";
  import TextField from "../../components/ui/TextField.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { toMessage } from "../../utils/errors";
  import SettingsNavigation from "../../components/ui/SettingsNavigation.svelte";
  import CloseButton from "../../components/ui/CloseButton.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import GameArtworkPanel from "./GameArtworkPanel.svelte";
  import PlaytimePanel from "./PlaytimePanel.svelte";
  import GameDetailsPanel from "./GameDetailsPanel.svelte";
  import ExecutablePanel from "./ExecutablePanel.svelte";
  import GameSettingsPanel from "./GameSettingsPanel.svelte";
  import SteamAccountPanel from "./SteamAccountPanel.svelte";
  import SteamLaunchSettings from "./SteamLaunchSettings.svelte";
  import { launchStateByGame } from "../../stores/launch";
  import { appInfo } from "../../stores/app-info";

  let { game, onClose, onRemoved }: {
    game: Game;
    onClose: () => void;
    onRemoved: (removed: Game) => void;
  } = $props();

  const steamManaged = $derived(game.steamInstallPath !== null);
  const allCategories = $derived([
    { id: "general", label: t("Generali", $language), icon: "settings" },
    { id: "locations", label: t("Posizioni", $language), icon: "folder" },
    { id: "customization", label: t("Personalizzazione", $language), icon: "image" },
    { id: "compatibility", label: t("Compatibilità", $language), icon: "wrench" },
    { id: "danger", label: t("Zona pericolosa", $language), icon: "warning" },
  ] satisfies { id: string; label: string; icon: "settings" | "folder" | "image" | "wrench" | "warning" }[]);
  const categories = $derived((steamManaged || $appInfo.data.platform !== "linux" || (game.executablePath !== null && !/\.exe$/i.test(game.executablePath)))
    ? allCategories.filter((category) => category.id !== "compatibility")
    : allCategories);
  let active = $state("general");
  const selectedCategory = $derived(categories.some(category => category.id === active) ? active : "general");
  let shortcutWarning = $state<string | null>(null);
  let shortcutError = $state<string | null>(null);
  let shortcutPath = $state<string | null>(null);
  let shortcutPending = $state(false);
  const running = $derived(($launchStateByGame.get(game.id)?.status ?? "idle") !== "idle");
  let destinationDraft = $state("");
  let transferPending = $state(false);
  let transferWarning = $state<string | null>(null);
  let transferError = $state<string | null>(null);
  let transferDone = $state(false);

  async function browseTransfer(): Promise<void> {
    try {
      const selected = await pickGameDirectory(destinationDraft);
      if (selected !== null) destinationDraft = selected;
    } catch (error) { transferError = toMessage(error); }
  }

  async function moveGame(): Promise<void> {
    transferPending = true;
    transferError = null;
    transferWarning = null;
    transferDone = false;
    try {
      const result = await transferGame(game.id, destinationDraft.trim());
      reconcileGame(result.game);
      transferWarning = result.warning;
      destinationDraft = "";
      transferDone = true;
    } catch (error) { transferError = toMessage(error); }
    finally { transferPending = false; }
  }

  async function addDesktopShortcut(): Promise<void> {
    shortcutPending = true;
    shortcutError = null;
    shortcutWarning = null;
    try {
      const result = await createDesktopShortcut(game.id);
      shortcutPath = result.path;
      shortcutWarning = result.warning;
    }
    catch (error) { shortcutError = toMessage(error); }
    finally { shortcutPending = false; }
  }

</script>

<Dialog open title={game.name} size="wide" flush {onClose}>
  {#snippet actions(close)}
    <CloseButton label={t("Chiudi le impostazioni", $language)} onClick={close} />
  {/snippet}

  <div class="flex min-h-0 flex-1">
    <SettingsNavigation {categories} selected={selectedCategory} label={t("Sezioni delle impostazioni del gioco", $language)} onSelect={id => (active = id)} />
    <div class="min-w-0 flex-1 overflow-y-auto p-6">
      {#if selectedCategory === "general"}
        <div class="flex flex-col gap-4">
          <GameDetailsPanel {game} {onRemoved} />
          <Panel title={t("Collegamento", $language)}>
            <Button label={shortcutPending ? t("Creazione...", $language) : t("Aggiungi collegamento desktop", $language)}
              disabled={shortcutPending} onClick={() => void addDesktopShortcut()} />
            {#if shortcutPath}<p class="mt-2 break-all text-sm text-legio-success" role="status">{t("Collegamento creato: ", $language)}{shortcutPath}</p>{/if}
            {#if shortcutWarning}<ErrorBanner message={shortcutWarning} tone="warning" />{/if}
            {#if shortcutError}<ErrorBanner message={shortcutError} />{/if}
          </Panel>
          {#if steamManaged}
            {#key game.id}<SteamAccountPanel {game} />{/key}
            {#key `${game.id}:${shortcutPath ?? ""}`}<SteamLaunchSettings {game} />{/key}
          {:else if game.executablePath !== null}
            {#key game.id}<GameSettingsPanel {game} section="launch" />{/key}
          {:else}
            <Panel title={t("Opzioni di avvio", $language)}>
              <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Seleziona prima un eseguibile nella sezione Posizioni.", $language)}</p>
            </Panel>
          {/if}
        </div>
      {:else if selectedCategory === "locations"}
        <div class="flex flex-col gap-4">
          {#if !steamManaged}<ExecutablePanel {game} />{:else}
            <Panel title={t("Installazione Steam", $language)}>
              <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Steam gestisce il percorso di installazione per questo gioco.", $language)}</p>
              <p class="break-all text-sm text-zinc-200 light:text-zinc-800">{game.steamInstallPath}</p>
            </Panel>
          {/if}
          {#if game.executablePath !== null}
            {#key game.id}<GameSettingsPanel {game} section="locations" />{/key}
          {/if}
          {#if !steamManaged && game.executablePath !== null}
            <Panel title={t("Trasferisci gioco", $language)}>
              <div class="flex flex-col gap-3">
                <TextField id="transfer-destination" label={t("Cartella di destinazione", $language)} value={destinationDraft}
                  placeholder={t("Seleziona la libreria di destinazione", $language)} oninput={(value) => (destinationDraft = value)} />
                <Button label={t("Sfoglia destinazione...", $language)} variant="secondary" disabled={transferPending}
                  onClick={() => void browseTransfer()} />
                <Button label={transferPending ? t("Trasferimento...", $language) : t("Trasferisci gioco", $language)}
                  disabled={running || transferPending || destinationDraft.trim().length === 0} onClick={() => void moveGame()} />
                {#if running}<p class="text-sm text-zinc-400">{t("Chiudi il gioco prima di trasferirlo.", $language)}</p>{/if}
                {#if transferDone}<p class="text-sm text-legio-success" role="status">{t("Gioco trasferito.", $language)}</p>{/if}
                {#if transferWarning}<ErrorBanner message={transferWarning} tone="warning" />{/if}
                {#if transferError}<ErrorBanner message={transferError} />{/if}
              </div>
            </Panel>
          {/if}
        </div>
      {:else if selectedCategory === "customization"}
        <GameArtworkPanel {game} />
      {:else if selectedCategory === "compatibility"}
        {#if game.executablePath !== null}
          {#key game.id}<GameSettingsPanel {game} section="compatibility" />{/key}
        {:else}
          <Panel title={t("Compatibilità", $language)}>
            <p class="text-sm text-zinc-400 light:text-zinc-600">{t("Seleziona prima un eseguibile per configurare il runner e gli argomenti di avvio.", $language)}</p>
          </Panel>
        {/if}
      {:else}
        <div class="flex flex-col gap-4">
          <PlaytimePanel {game} />
          <GameDetailsPanel {game} {onRemoved} {running} section="danger" />
        </div>
      {/if}
    </div>
  </div>
</Dialog>
