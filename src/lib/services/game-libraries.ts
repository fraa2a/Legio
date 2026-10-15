import { invoke } from "./invoke";

export interface GameLibrary {
  id: string;
  root: string;
  warning: string | null;
}

export const listGameLibraries = () => invoke<GameLibrary[]>("get_game_libraries");
export const importGameLibrary = (directory: string) => invoke<void>("import_game_library", { directory });
export const repairGameLibrary = (directory: string) => invoke<void>("repair_game_library", { directory });
