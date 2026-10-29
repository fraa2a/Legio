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
export const removeGameLibrary = (id: string) => invoke<void>("remove_game_library", { id });
export const repairGameLibrary = (directory: string) => invoke<void>("repair_game_library", { directory });
