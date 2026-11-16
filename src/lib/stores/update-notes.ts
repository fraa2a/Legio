import { get, writable } from "svelte/store";
import { t } from "../i18n";
import { appInfo } from "./app-info";
import { settings, updateSettings } from "./settings";
import { showToast } from "./toast";

export type ReleaseNotes = { version: string; notes: string };

export const MAX_NOTES_CHARS = 20_000;

export const openedNotes = writable<ReleaseNotes | null>(null);

export const openReleaseNotes = (notes: ReleaseNotes): void => openedNotes.set(notes);
export const closeReleaseNotes = (): void => openedNotes.set(null);

export async function rememberPendingNotes(version: string, notes: string | null): Promise<void> {
  await updateSettings({ pendingUpdateNotes: notes ? { version, notes } : null });
}

// Announces a version change once. The first run with no recorded version only records it.
export async function announceInstalledVersion(): Promise<void> {
  const current = get(appInfo).data.version;
  const { lastSeenVersion, pendingUpdateNotes } = get(settings).data;
  if (!current || lastSeenVersion === current) return;
  await updateSettings({ lastSeenVersion: current, pendingUpdateNotes: null });
  if (lastSeenVersion === null) return;
  const notes = pendingUpdateNotes?.version === current ? pendingUpdateNotes : null;
  const message = t("Legio è stato aggiornato alla versione {0}.", undefined, [current]);
  if (notes === null) showToast(message, "info");
  else showToast(message, "info", { onRetry: () => openReleaseNotes(notes), retryLabel: t("Cosa c'è di nuovo", undefined), persistent: true });
}
