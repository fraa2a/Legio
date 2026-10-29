import { t, type Language } from "../../i18n";
import type { GameLibrary } from "../../services/game-libraries";
import { formatBytes } from "../../utils/format";

export function formatLibraryDetails(library: GameLibrary, selected: Language): string {
  const count = library.games.length === 1 ? t("1 gioco", selected) : t("{0} giochi", selected, [String(library.games.length)]);
  if (library.freeBytes === null) return `${count} · ${t("Cartella non raggiungibile", selected)}`;
  return `${count} · ${t("{0} liberi", selected, [formatBytes(library.freeBytes, selected)])}`;
}
