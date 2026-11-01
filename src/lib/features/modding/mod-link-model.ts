import { t, type Language } from "../../i18n";
import type { LinkHandler, ModLinkCandidate, ModLinkProfile, ModLinkResolution } from "../../services/mod-links";

// The active profile, else the only one; with several and none active the user has to choose.
export function defaultLinkProfileId(candidate: ModLinkCandidate): string | null {
  const active = candidate.profiles.find((profile) => profile.id === candidate.activeProfileId);
  if (active !== undefined) return active.id;
  return candidate.profiles.length === 1 ? (candidate.profiles[0]?.id ?? null) : null;
}

export function linkProfile(candidate: ModLinkCandidate | null, profileId: string | null): ModLinkProfile | null {
  return candidate?.profiles.find((profile) => profile.id === profileId) ?? null;
}

export function linkProfileOptions(candidate: ModLinkCandidate, selected: Language): { value: string; label: string }[] {
  return candidate.profiles.map((profile) => ({
    value: profile.id,
    label: profile.id === candidate.activeProfileId ? t("{0} (attivo)", selected, [profile.name]) : profile.name,
  }));
}

export function linkGameOptions(candidates: readonly ModLinkCandidate[]): { value: string; label: string }[] {
  return candidates.map((candidate) => ({ value: candidate.gameId, label: candidate.name }));
}

export interface CatalogRefresh { community: string; force: boolean }

// Catalogs to download before the link can be trusted: never synced ones, and synced ones without the version.
// A forced refresh is only worth it when the catalog knows the package, so the version is probably newer than the local copy.
export function catalogRefreshes(resolution: ModLinkResolution): CatalogRefresh[] {
  const force = new Map<string, boolean>();
  for (const game of resolution.unsynced) force.set(game.ecosystemGameId, false);
  for (const game of resolution.missing) force.set(game.ecosystemGameId, (force.get(game.ecosystemGameId) ?? false) || game.packageKnown);
  return [...force].map(([community, forced]) => ({ community, force: forced }));
}

// Games whose catalog has the package but not this version, even after the refresh.
export function missingVersionNames(resolution: ModLinkResolution): string[] {
  return resolution.missing.filter((game) => game.packageKnown).map((game) => game.name);
}

// What to tell the user after turning the option off, when Legio did not end up as the only one out of the picture.
export function linkReleaseNote(handler: LinkHandler, selected: Language): string | null {
  switch (handler) {
    case "legio":
      return t("Non è stato possibile rimuovere la registrazione di Legio: i link di Thunderstore si aprono ancora con Legio.", selected);
    case "other":
      return t("I link di Thunderstore sono ancora gestiti da un altro programma, che Legio non ha modificato.", selected);
    case "nobody":
      return null;
  }
}

export type LinkInstallState = "ready" | "running" | "busy" | "no-profile" | "installed";

export function linkInstallState(
  profile: ModLinkProfile | null,
  version: string,
  running: boolean,
  busy: boolean,
): LinkInstallState {
  if (running) return "running";
  if (busy) return "busy";
  if (profile === null) return "no-profile";
  return profile.installedVersion === version ? "installed" : "ready";
}

export function linkInstallNote(state: LinkInstallState, profile: ModLinkProfile | null, selected: Language): string | null {
  switch (state) {
    case "running":
      return t("Il gioco è in esecuzione: chiudilo per installare, attivare o rimuovere mod e per cambiare profilo.", selected);
    case "busy":
      return t("Un'altra installazione è in corso per questo profilo.", selected);
    case "no-profile":
      return t("Scegli il profilo in cui installare la mod.", selected);
    case "installed":
      return t("Questa versione è già installata nel profilo.", selected);
    case "ready":
      return profile?.installedVersion == null
        ? null
        : t("Nel profilo è installata la versione {0}: verrà sostituita.", selected, [profile.installedVersion]);
  }
}
