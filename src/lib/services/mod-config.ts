import { invoke } from "./invoke";

export interface ConfigFileEntry {
  relativePath: string;
  size: number;
  modified: number;
}

export interface ConfigEntry {
  key: string;
  value: string;
  description: string | null;
  settingType: string | null;
  defaultValue: string | null;
  acceptableValues: string[];
  acceptableRange: [string, string] | null;
  line: number;
}

export interface BepInExConfig {
  sections: { name: string; entries: ConfigEntry[] }[];
}

export interface ModConfigFile {
  relativePath: string;
  text: string;
  bepinex: BepInExConfig | null;
}

export interface ConfigEdit {
  section: string;
  key: string;
  value: string;
}

export const listModConfigFiles = (gameId: string, profileId: string) =>
  invoke<ConfigFileEntry[]>("list_mod_config_files", { gameId, profileId });

export const readModConfigFile = (gameId: string, profileId: string, relativePath: string) =>
  invoke<ModConfigFile>("read_mod_config_file", { gameId, profileId, relativePath });

export const saveModConfigText = (gameId: string, profileId: string, relativePath: string, text: string) =>
  invoke<void>("save_mod_config_text", { gameId, profileId, relativePath, text });

export const saveModConfigEntries = (gameId: string, profileId: string, relativePath: string, edits: ConfigEdit[]) =>
  invoke<void>("save_mod_config_entries", { gameId, profileId, relativePath, edits });
