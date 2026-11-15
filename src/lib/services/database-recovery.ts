import { relaunch } from "@tauri-apps/plugin-process";
import { invoke } from "./invoke";

export interface DatabaseBackup {
  fileName: string;
  schemaVersion: number;
  createdAt: number;
}

export const getDatabaseFailure = () => invoke<string | null>("get_database_failure");
export const listDatabaseBackups = () => invoke<DatabaseBackup[]>("list_database_backups");
export const openDataFolder = () => invoke<void>("open_data_folder");
export const restoreDatabaseBackup = (fileName: string) => invoke<void>("restore_database_backup", { fileName });
export const resetLocalDatabase = () => invoke<void>("reset_local_database");
export const restartApp = () => relaunch();
