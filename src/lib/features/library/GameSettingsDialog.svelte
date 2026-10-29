<script lang="ts">
  import { onMount } from "svelte";
  import { t, language } from "../../i18n";
  import { createDesktopShortcut, transferGame, type Game } from "../../services/local-state";
  import type { GameLibrary } from "../../services/game-libraries";
  import { reconcileGame } from "../../stores/games";
  import { gameLibraries } from "../../stores/game-libraries";
  import LibraryPickerDialog from "./LibraryPickerDialog.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import { toMessage } from "../../utils/errors";
  import SettingsNavigation from "../../components/ui/SettingsNavigation.svelte";
  import CloseButton from "../../components/ui/CloseButton.svelte";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Callout from "../../components/ui/Callout.svelte";
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
  let transferPickerOpen = $state(false);
  let transferPending = $state(false);
  let transferWarning = $state<string | null>(null);
  let transferError = $state<string | null>(null);
  let transferDone = $state(false);

  const currentLibrary = $derived($gameLibraries.data.find(library => library.games.some(entry => entry.id === game.id)) ?? null);
  onMount(() => { void gameLibraries.load(); });

  async function moveGame(library: GameLibrary): Promise<void> {
    transferPickerOpen = false;
    transferPending = true;
    transferError = null;
    transferWarning = null;
    transferDone = false;
    try {
      const result = await transferGame(game.id, library.root);
      reconcileGame(result.game);
      transferWarning = result.warning;
      transferDone = true;
    } catch (error) { transferError = toMessage(error); }
    finally { transferPending = false; void gameLibraries.reload(); }
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
            {#if shortcutPath}<Callout tone="success" variant="inline" class="mt-2 break-all" message={t("Collegamento creato: {0}", $language, [shortcutPath])} />{/if}
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
                <p class="break-all text-sm text-zinc-400 light:text-zinc-600">{currentLibrary === null ? t("Questo gioco non si trova in una libreria di Legio.", $language) : t("Libreria attuale: {0}", $language, [currentLibrary.root])}</p>
                <div><Button label={transferPending ? t("Trasferimento...", $language) : t("Sposta in un'altra libreria...", $language)}
                  disabled={running || transferPending} onClick={() => (transferPickerOpen = true)} /></div>
                {#if running}<p class="text-sm text-zinc-400">{t("Chiudi il gioco prima di trasferirlo.", $language)}</p>{/if}
                {#if transferDone}<Callout tone="success" variant="inline" message={t("Gioco trasferito.", $language)} />{/if}
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

<LibraryPickerDialog
  open={transferPickerOpen}
  title={t("Sposta {0}", $language, [game.name])}
  confirmLabel={t("Sposta qui", $language)}
  excludeId={currentLibrary?.id ?? null}
  onSelect={(library) => void moveGame(library)}
  onClose={() => (transferPickerOpen = false)}
/>
