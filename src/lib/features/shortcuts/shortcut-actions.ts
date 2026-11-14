import { get } from "svelte/store";
import { paletteOpen } from "../../stores/command-palette";
import { games } from "../../stores/games";
import { launchStateByGame, pendingGameId, playGame } from "../../stores/launch";
import { activeSection, sections, selectSection, selectedGameId, selectedModGame } from "../../stores/navigation";
import { shortcutsOpen } from "../../stores/shortcuts";
import { matchShortcut, shortcutScope, type ShortcutAction } from "./shortcut-model";

const searchFieldId = "title-bar-search";

function focusSearch(): void {
  const field = document.getElementById(searchFieldId);
  if (field instanceof HTMLElement && field.offsetParent !== null) field.focus();
  else paletteOpen.set(true);
}

// Enter plays only on the game page, the one place with a single selected game.
function playOpenGame(): boolean {
  if (get(activeSection) !== "library" || get(selectedModGame) !== null) return false;
  const id = get(selectedGameId);
  const game = id === null ? undefined : get(games).data.find((entry) => entry.id === id);
  if (game === undefined || (game.steamInstallPath === null && game.executablePath === null)) return false;
  if ((get(launchStateByGame).get(game.id)?.status ?? "idle") !== "idle" || get(pendingGameId) === game.id) return false;
  void playGame(game);
  return true;
}

function run(action: ShortcutAction): boolean {
  switch (action.type) {
    case "palette": paletteOpen.update((open) => !open); return true;
    case "section": selectSection(sections[action.index]); return true;
    case "search": focusSearch(); return true;
    case "play": return playOpenGame();
    case "help": shortcutsOpen.set(true); return true;
  }
}

export function handleShortcutKey(event: KeyboardEvent): void {
  if (event.defaultPrevented || event.repeat) return;
  const dialogOpen = document.querySelector("dialog[open], [aria-modal='true']") !== null;
  const action = matchShortcut(event, shortcutScope(event.target, dialogOpen));
  if (action !== null && run(action)) event.preventDefault();
}
