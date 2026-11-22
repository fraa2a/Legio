import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "./invoke";

export interface GameLibrary {
  id: string;
  root: string;
  warning: string | null;
  isDefault: boolean;
  freeBytes: number | null;
  games: { id: string; name: string }[];
}

export const listGameLibraries = () => invoke<GameLibrary[]>("get_game_libraries");
export const addGameLibrary = (directory: string) => invoke<string>("add_game_library", { directory });
export type LibraryRemovalRequest = { kind: "uninstall" } | { kind: "move"; targetId: string };

export interface LibraryRemovalOutcome {
  warnings: string[];
  /** The game that stopped the removal. The library and the games after it are left untouched. */
  failure: { game: string; message: string } | null;
}

export interface LibraryRemovalProgress { libraryId: string; done: number; current: string | null }

// Moves or uninstalls the library's games in the backend, then forgets the library.
export const removeGameLibrary = (id: string, removal: LibraryRemovalRequest) =>
  invoke<LibraryRemovalOutcome>("remove_game_library", { id, removal });
export const onLibraryRemovalProgress = (callback: (progress: LibraryRemovalProgress) => void): Promise<UnlistenFn> =>
  listen<LibraryRemovalProgress>("game-library-removal-progress", (event) => callback(event.payload));
export const repairGameLibrary = (directory: string) => invoke<void>("repair_game_library", { directory });
