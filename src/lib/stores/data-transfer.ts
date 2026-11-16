import { get, writable } from "svelte/store";
import { exportUserData, stageUserDataImport } from "../services/data-transfer";
import { restartApp } from "../services/database-recovery";
import { pickDataExportPath, pickDataImportFile } from "../services/dialog";
import { toMessage } from "../utils/errors";

export type DataTransferAction = "export" | "import";

interface DataTransferState {
  pending: DataTransferAction | null;
  importPath: string | null;
  error: string | null;
}

export const dataTransfer = writable<DataTransferState>({ pending: null, importPath: null, error: null });

export function exportFileName(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `legio-dati-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}.zip`;
}

/** Asks for a destination and writes the export there. Resolves true once the file is saved. */
export async function exportData(): Promise<boolean> {
  if (get(dataTransfer).pending !== null) return false;
  dataTransfer.update((state) => ({ ...state, pending: "export", error: null }));
  try {
    const path = await pickDataExportPath(exportFileName(new Date()));
    if (path === null) return false;
    await exportUserData(path);
    return true;
  } catch (error) {
    dataTransfer.update((state) => ({ ...state, error: toMessage(error) }));
    return false;
  } finally {
    dataTransfer.update((state) => ({ ...state, pending: null }));
  }
}

export async function chooseImportFile(): Promise<void> {
  if (get(dataTransfer).pending !== null) return;
  try {
    const path = await pickDataImportFile();
    if (path !== null) dataTransfer.update((state) => ({ ...state, importPath: path, error: null }));
  } catch (error) {
    dataTransfer.update((state) => ({ ...state, error: toMessage(error) }));
  }
}

export function cancelImport(): void {
  if (get(dataTransfer).pending === null) dataTransfer.update((state) => ({ ...state, importPath: null, error: null }));
}

// The open database cannot be replaced, so the staged import is applied by the restart.
export async function confirmImport(): Promise<void> {
  const { importPath, pending } = get(dataTransfer);
  if (importPath === null || pending !== null) return;
  dataTransfer.update((state) => ({ ...state, pending: "import", error: null }));
  try {
    await stageUserDataImport(importPath);
    await restartApp();
  } catch (error) {
    dataTransfer.update((state) => ({ ...state, pending: null, error: toMessage(error) }));
  }
}
