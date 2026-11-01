import { t, type Language } from "../../i18n";
import type { ModLinkCandidate, ModLinkProfile, ModLinkResolution } from "../../services/mod-links";

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

// Ecosystem games whose catalog has to be downloaded before the link can be resolved.
export function unsyncedCommunities(resolution: ModLinkResolution): string[] {
  return [...new Set(resolution.unsynced.map((game) => game.ecosystemGameId))];
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
