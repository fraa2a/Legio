import type { Settings } from "../../services/local-state";

// Strings are translation keys. Keep in sync with _meta/docs/PRIVACY.md.
export type NetworkSwitch = "newsEnabled" | "automaticUpdateCheck" | "discordPresence";

export interface NetworkDestination {
  id: string;
  name: string;
  hosts: string;
  purpose: string;
  when: string;
  sends: string;
  // Without a switch the entry is either needed by the app or only used on demand.
  kind: "essential" | "on-demand" | "switch";
  switch?: NetworkSwitch;
}

export const networkDestinations: readonly NetworkDestination[] = [
  {
    id: "steam",
    name: "Steam",
    hosts: "store.steampowered.com, steamstatic.com, server Steam",
    purpose: "Schede dei giochi, copertine e controllo della connessione.",
    when: "All'avvio e quando una schermata mostra un gioco.",
    sends: "ID del gioco e lingua. Accesso anonimo, mai il tuo account.",
    kind: "essential",
  },
  {
    id: "catalog",
    name: "Catalogo Hydra",
    hosts: "hydra-api-us-east-1.losbroxas.org",
    purpose: "Ricerca dei titoli nello Store e abbinamento con le sorgenti.",
    when: "Quando cerchi nello Store o colleghi una sorgente.",
    sends: "Il testo cercato o il titolo del gioco.",
    kind: "essential",
  },
  {
    id: "protondb",
    name: "ProtonDB",
    hosts: "www.protondb.com",
    purpose: "Valutazione di compatibilità su Linux mostrata nella pagina del gioco.",
    when: "Solo su Linux, quando apri la pagina di un gioco. La risposta resta in cache per 7 giorni.",
    sends: "ID del gioco.",
    kind: "on-demand",
  },
  {
    id: "news",
    name: "Novità di Legio",
    hosts: "source.taxphobia.top",
    purpose: "Articoli mostrati nella home.",
    when: "All'avvio e ogni 15 minuti mentre la home è visibile.",
    sends: "Solo la richiesta HTTP.",
    kind: "switch",
    switch: "newsEnabled",
  },
  {
    id: "updates",
    name: "Aggiornamenti dell'app",
    hosts: "github.com",
    purpose: "Cerca nuove versioni di Legio. Il download parte solo su tua richiesta.",
    when: "All'avvio della versione installata e dal pulsante Controlla aggiornamenti.",
    sends: "Solo la richiesta HTTP.",
    kind: "switch",
    switch: "automaticUpdateCheck",
  },
  {
    id: "discord",
    name: "Discord",
    hosts: "Discord desktop sul tuo computer (nessuna connessione di rete di Legio)",
    purpose: "Mostra il gioco in corso nel tuo profilo Discord.",
    when: "Mentre giochi.",
    sends: "Nome del gioco, tempo di sessione e immagini. Discord le carica da Steam e GitHub.",
    kind: "switch",
    switch: "discordPresence",
  },
  {
    id: "sources",
    name: "Sorgenti di download",
    hosts: "Gli indirizzi che aggiungi tu",
    purpose: "Elenco dei giochi disponibili e aggiornamenti.",
    when: "All'avvio e quando aggiorni la sorgente. Nessuna sorgente è inclusa.",
    sends: "Solo la richiesta HTTP.",
    kind: "on-demand",
  },
  {
    id: "downloads",
    name: "Host dei download",
    hosts: "gofile.io, buzzheavier.com, 1fichier.com, mediafire.com, pixeldrain.com, fuckingfast.co, rootz.so, cdn.taxphobia.top e gli host delle tue sorgenti",
    purpose: "Verifica dei link e scaricamento dei file di gioco.",
    when: "Quando apri un gioco con download e quando avvii un download.",
    sends: "L'indirizzo del file. GoFile crea una sessione ospite anonima.",
    kind: "on-demand",
  },
  {
    id: "mods",
    name: "Thunderstore",
    hosts: "thunderstore.io, raw.githubusercontent.com",
    purpose: "Catalogo e file delle mod, elenco dei pacchetti esclusi, codici profilo.",
    when: "Solo quando usi le funzioni mod.",
    sends: "Pacchetti richiesti. Un codice profilo creato da te carica il profilo.",
    kind: "on-demand",
  },
];

export function isNetworkSwitchOn(settings: Settings, name: NetworkSwitch): boolean {
  return name === "discordPresence" ? settings.discordPresence.enabled : settings[name];
}

export function networkSwitchChange(name: NetworkSwitch, enabled: boolean): Partial<Settings> {
  return name === "discordPresence" ? { discordPresence: { enabled } } : { [name]: enabled };
}
