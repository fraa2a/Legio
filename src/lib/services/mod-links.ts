import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "./invoke";

export interface ModLinkTarget { fullName: string; owner: string; name: string; version: string }

// Either the package a `ror2mm://` link asks for, or why the link was refused.
export interface ModLinkRequest { target: ModLinkTarget | null; error: string | null }

export interface ModLinkProfile { id: string; name: string; installedVersion: string | null }

export interface ModLinkCandidate {
  gameId: string;
  name: string;
  ecosystemGameId: string;
  activeProfileId: string | null;
  profiles: ModLinkProfile[];
}

export interface UnsyncedGame { gameId: string; name: string; ecosystemGameId: string }

export interface ModLinkResolution {
  package: { description: string; icon: string } | null;
  candidates: ModLinkCandidate[];
  unsynced: UnsyncedGame[];
}

export const takeModLinks = () => invoke<ModLinkRequest[]>("take_mod_links");
export const resolveModLink = (fullName: string, version: string) =>
  invoke<ModLinkResolution>("resolve_mod_link", { fullName, version });

export function observeModLinks(callback: () => void): Promise<UnlistenFn> {
  return listen("legio:mod-link", () => callback());
}
