import type { SteamGridDbListing, SteamGridDbSlot } from "../../services/steamgriddb";

export interface SteamGridDbTab {
  id: SteamGridDbListing;
  label: string;
  slot: SteamGridDbSlot;
}

// Heroes and landscape covers fill the banner slot; icons fill the icon slot.
// SteamGridDB logos and portrait covers have no custom artwork slot yet.
export const steamGridDbTabs: readonly SteamGridDbTab[] = [
  { id: "hero", label: "Hero", slot: "banner" },
  { id: "grid", label: "Copertine", slot: "banner" },
  { id: "icon", label: "Icone", slot: "icon" },
];

export function steamGridDbSlot(listing: string): SteamGridDbSlot | null {
  return steamGridDbTabs.find((tab) => tab.id === listing)?.slot ?? null;
}

export type PickerView = "no-key" | "loading" | "no-games" | "no-images" | "error" | "ready";

export function pickerView(state: { hasKey: boolean | null; loading: boolean; error: string | null; games: number; images: number; searched: boolean }): PickerView {
  if (state.hasKey === false) return "no-key";
  if (state.error !== null) return "error";
  if (state.hasKey === null || state.loading) return "loading";
  if (state.searched && state.games === 0) return "no-games";
  if (state.images === 0 && state.games > 0) return "no-images";
  return "ready";
}

// A key can only be saved when something was typed; the saved key is never shown again.
export function canSaveKey(input: string): boolean {
  return input.trim().length >= 8;
}
