import { writable } from "svelte/store";
import { getDiskUsage, getLibraryUsage, onDiskUsageChanged, refreshDiskUsage, type DiskUsageSnapshot, type LibraryUsage } from "../services/disk-usage";
import { createResource } from "./resource";

export const diskUsage = createResource<DiskUsageSnapshot>({ games: [], computing: false }, getDiskUsage);
export const libraryUsage = createResource<LibraryUsage[]>([], getLibraryUsage);
export const diskUsageDialogOpen = writable(false);

/** Asks the backend to measure, for one game or all of them. Sizes arrive through the change event. */
export async function requestDiskUsage(gameId: string | null = null): Promise<void> {
  await refreshDiskUsage(gameId);
  await diskUsage.reload();
}

/** Keeps the stores current while a view is mounted. Returns the function that stops it. */
export function watchDiskUsage(includeLibraries: boolean): () => void {
  const refresh = () => void Promise.all([diskUsage.reload(), includeLibraries ? libraryUsage.reload() : Promise.resolve()]);
  const listening = onDiskUsageChanged(refresh);
  refresh();
  return () => void listening.then((unlisten) => unlisten());
}
