import { get, writable } from "svelte/store";
import {
  getDatabaseFailure,
  listDatabaseBackups,
  openDataFolder,
  resetLocalDatabase,
  restartApp,
  restoreDatabaseBackup,
  type DatabaseBackup,
} from "../services/database-recovery";
import { toMessage } from "../utils/errors";
import { createResource } from "./resource";

export const databaseFailure = createResource<string | null>(null, getDatabaseFailure);

export type RecoveryAction = "restore" | "reset";

interface RecoveryState {
  backups: DatabaseBackup[];
  backupsLoaded: boolean;
  selected: string;
  pending: RecoveryAction | null;
  error: string | null;
}

export const databaseRecovery = writable<RecoveryState>({ backups: [], backupsLoaded: false, selected: "", pending: null, error: null });

export async function loadDatabaseBackups(): Promise<void> {
  try {
    const backups = await listDatabaseBackups();
    // The backend lists the newest backup first.
    databaseRecovery.update((state) => ({ ...state, backups, backupsLoaded: true, selected: backups[0]?.fileName ?? "" }));
  } catch (error) {
    databaseRecovery.update((state) => ({ ...state, backupsLoaded: true, error: toMessage(error) }));
  }
}

export function selectDatabaseBackup(fileName: string): void {
  databaseRecovery.update((state) => ({ ...state, selected: fileName }));
}

// The database state is fixed at startup, so a replaced file takes effect after a restart.
async function replaceDatabase(action: RecoveryAction, replace: () => Promise<void>): Promise<void> {
  if (get(databaseRecovery).pending !== null) return;
  databaseRecovery.update((state) => ({ ...state, pending: action, error: null }));
  try {
    await replace();
    await restartApp();
  } catch (error) {
    databaseRecovery.update((state) => ({ ...state, pending: null, error: toMessage(error) }));
  }
}

export function restoreSelectedBackup(): Promise<void> {
  const { selected } = get(databaseRecovery);
  if (selected === "") return Promise.resolve();
  return replaceDatabase("restore", () => restoreDatabaseBackup(selected));
}

export function startWithNewDatabase(): Promise<void> {
  return replaceDatabase("reset", resetLocalDatabase);
}

export async function openDatabaseFolder(): Promise<void> {
  try {
    await openDataFolder();
  } catch (error) {
    databaseRecovery.update((state) => ({ ...state, error: toMessage(error) }));
  }
}
