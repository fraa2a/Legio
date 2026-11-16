import { t, type Language } from "../../i18n";
import { playStatuses, type PlayStatus } from "../../services/library-organization";
import type { LibrarySortKey } from "../../services/local-state";
import type { LibraryOrigin } from "./library-organize";

const statusLabels: Record<PlayStatus, string> = {
  none: "Nessuno stato",
  backlog: "Da giocare",
  playing: "Sto giocando",
  completed: "Completato",
  onHold: "In sospeso",
  dropped: "Abbandonato",
};

const sortLabels: Record<LibrarySortKey, string> = {
  name: "Nome",
  playtime: "Tempo di gioco",
  lastPlayed: "Ultima partita",
  added: "Aggiunti di recente",
  origin: "Origine",
};

export function playStatusLabel(status: PlayStatus, language: Language): string {
  return t(statusLabels[status], language);
}

export function playStatusOptions(language: Language): { value: PlayStatus; label: string }[] {
  return playStatuses.map((status) => ({ value: status, label: playStatusLabel(status, language) }));
}

export function sortLabel(key: LibrarySortKey, language: Language): string {
  return t(sortLabels[key], language);
}

export function originLabel(origin: LibraryOrigin, language: Language): string {
  return origin === "steam" ? "Steam" : t("Locale", language);
}

export function formatPlaytime(milliseconds: number): string {
  const hours = Math.floor(milliseconds / 3600000);
  if (hours >= 1) return `${hours}h`;
  return `${Math.floor(milliseconds / 60000)}m`;
}
