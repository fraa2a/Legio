import { get } from "svelte/store";
import { t } from "../i18n";
import { pickGameDirectory } from "../services/dialog";
import { addGameLibrary, listGameLibraries, onLibraryRemovalProgress, removeGameLibrary, type GameLibrary, type LibraryRemovalOutcome } from "../services/game-libraries";
import { games } from "./games";
import { createResource } from "./resource";

export const gameLibraries = createResource<GameLibrary[]>([], listGameLibraries);

export type LibraryRemoval = { kind: "uninstall" } | { kind: "move"; target: GameLibrary };

export type { LibraryRemovalOutcome } from "../services/game-libraries";

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

/** Empties the library in the backend, which stops at the first failed game, then forgets it. */
export async function removeLibrary(library: GameLibrary, removal: LibraryRemoval, onProgress: (done: number, current: string | null) => void): Promise<LibraryRemovalOutcome> {
  const unlisten = await onLibraryRemovalProgress((progress) => { if (progress.libraryId === library.id) onProgress(progress.done, progress.current); });
  try {
    return await removeGameLibrary(library.id, removal.kind === "move" ? { kind: "move", targetId: removal.target.id } : { kind: "uninstall" });
  } finally {
    unlisten();
    await Promise.all([games.reload(), gameLibraries.reload()]);
  }
}
