import { get } from "svelte/store";
import { t } from "../i18n";
import { pickGameDirectory } from "../services/dialog";
import { addGameLibrary, listGameLibraries, removeGameLibrary, type GameLibrary } from "../services/game-libraries";
import { getGameUninstallTarget, transferGame } from "../services/local-state";
import { toMessage } from "../utils/errors";
import { games, reconcileGame, uninstallGame } from "./games";
import { createResource } from "./resource";

export const gameLibraries = createResource<GameLibrary[]>([], listGameLibraries);

export type LibraryRemoval = { kind: "uninstall" } | { kind: "move"; target: GameLibrary };

export interface LibraryRemovalOutcome {
  warnings: string[];
  /** The game that stopped the removal. The library and the games after it are left untouched. */
  failure: { game: string; message: string } | null;
}

/** Whether the library is reachable and, when the size is known, has room for it. */
export function hasRoomFor(library: GameLibrary, requiredBytes: number | null): boolean {
  return library.freeBytes !== null && (requiredBytes === null || library.freeBytes >= requiredBytes);
}

/** Returns the only library when it can take the download, so no choice is needed. */
export async function onlyUsableLibrary(requiredBytes: number | null): Promise<GameLibrary | null> {
  await gameLibraries.reload();
  const libraries = get(gameLibraries).data;
  return libraries.length === 1 && hasRoomFor(libraries[0], requiredBytes) ? libraries[0] : null;
}

/** Asks for a folder and registers it as a library. Returns the library ID, or null when cancelled. */
export async function pickAndAddLibrary(): Promise<string | null> {
  const directory = await pickGameDirectory(null, t("Scegli dove creare la libreria", undefined));
  if (directory === null) return null;
  const id = await addGameLibrary(directory);
  // A folder with an existing catalog brings its games along.
  await Promise.all([gameLibraries.reload(), games.reload()]);
  return id;
}

/** Empties the library one game at a time, stopping at the first failure, then forgets it. */
export async function removeLibrary(library: GameLibrary, removal: LibraryRemoval, onProgress: (done: number, current: string | null) => void): Promise<LibraryRemovalOutcome> {
  const warnings: string[] = [];
  try {
    for (const [index, game] of library.games.entries()) {
      onProgress(index, game.name);
      try {
        if (removal.kind === "move") {
          const result = await transferGame(game.id, removal.target.root);
          reconcileGame(result.game);
          if (result.warning !== null) warnings.push(result.warning);
        } else {
          const directory = await getGameUninstallTarget(game.id);
          warnings.push(...(await uninstallGame(game.id, directory)).warnings);
        }
      } catch (error) {
        return { warnings, failure: { game: game.name, message: toMessage(error) } };
      }
    }
    onProgress(library.games.length, null);
    await removeGameLibrary(library.id);
    return { warnings, failure: null };
  } finally {
    await gameLibraries.reload();
  }
}
