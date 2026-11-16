import type { ProtonDbConfidence, ProtonDbSummary, ProtonDbTier } from "../../services/protondb";

// Labels are Italian i18n keys; callers pass them through `t`.
export const tierLabel: Record<ProtonDbTier, string> = {
  platinum: "Platino",
  gold: "Oro",
  silver: "Argento",
  bronze: "Bronzo",
  borked: "Non funziona",
  pending: "In attesa",
  unknown: "Sconosciuto",
};

export const tierMeaning: Record<ProtonDbTier, string> = {
  platinum: "Funziona perfettamente senza modifiche",
  gold: "Funziona perfettamente dopo piccole modifiche",
  silver: "Funziona con qualche problema minore",
  bronze: "Funziona, ma con problemi frequenti",
  borked: "Non parte o non è giocabile",
  pending: "Servono altre segnalazioni per una valutazione",
  unknown: "Valutazione non disponibile",
};

export const confidenceLabel: Record<ProtonDbConfidence, string> = {
  strong: "Alta",
  good: "Buona",
  moderate: "Media",
  low: "Bassa",
  inadequate: "Insufficiente",
  unknown: "Sconosciuta",
};

export type RatedTier = "platinum" | "gold" | "silver" | "bronze" | "borked";

export function isRatedTier(tier: ProtonDbTier | null | undefined): tier is RatedTier {
  return tier === "platinum" || tier === "gold" || tier === "silver" || tier === "bronze" || tier === "borked";
}

// The badge shows only a real rating, never pending or unknown.
export function badgeTier(summary: ProtonDbSummary | null): RatedTier | null {
  return summary !== null && isRatedTier(summary.tier) ? summary.tier : null;
}
