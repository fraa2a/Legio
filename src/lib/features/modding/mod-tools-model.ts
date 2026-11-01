import { t, type Language } from "../../i18n";
import type { ModCacheKind, ModCacheUsage } from "../../services/modding";
import { describeModdingError } from "./modding-model";

export interface CacheRow {
  kind: ModCacheKind;
  title: string;
  description: string;
  confirmMessage: string;
  bytes: number;
}

export function cacheRows(usage: ModCacheUsage, selected?: Language): CacheRow[] {
  return [
    {
      kind: "packages",
      title: t("Pacchetti scaricati", selected),
      description: t("Le mod scaricate da Thunderstore. I profili non cambiano: una mod disattivata viene scaricata di nuovo quando la riattivi.", selected),
      confirmMessage: t("Eliminare i pacchetti scaricati? Le mod già installate nei profili restano disponibili.", selected),
      bytes: usage.packagesBytes,
    },
    {
      kind: "catalog",
      title: t("Catalogo delle mod", selected),
      description: t("L'elenco locale delle mod di Thunderstore. Viene scaricato di nuovo alla prossima apertura del catalogo.", selected),
      confirmMessage: t("Eliminare il catalogo delle mod? Verrà scaricato di nuovo alla prossima apertura.", selected),
      bytes: usage.catalogBytes,
    },
    {
      kind: "icons",
      title: t("Icone delle mod", selected),
      description: t("Le icone dei pacchetti, scaricate di nuovo quando servono.", selected),
      confirmMessage: t("Eliminare le icone delle mod? Verranno scaricate di nuovo quando servono.", selected),
      bytes: usage.iconsBytes,
    },
  ];
}

export function clearedMessage(kind: ModCacheKind, selected?: Language): string {
  switch (kind) {
    case "packages": return t("Pacchetti scaricati eliminati.", selected);
    case "catalog": return t("Catalogo delle mod eliminato.", selected);
    case "icons": return t("Icone delle mod eliminate.", selected);
  }
}

// Backend errors are English; these are the ones the tools can raise on their own.
const toolErrors: Record<string, string> = {
  "Mods are being installed or changed. Wait for it to finish and try again": "Una mod è in corso di installazione o modifica. Attendi il termine e riprova.",
  "The mod catalog is being updated. Try again when it finishes": "Il catalogo delle mod è in aggiornamento. Riprova al termine.",
  "No BepInEx log found: launch the game with this profile at least once": "Nessun log trovato: avvia il gioco con questo profilo almeno una volta.",
  "This game has no active mod profile": "Questo gioco non ha un profilo mod attivo.",
};

export function describeToolsError(message: string, selected?: Language): string {
  const known = toolErrors[message.trim().replace(/\.$/, "")];
  return known === undefined ? describeModdingError(message, selected) : t(known, selected);
}
