export type GameLinkKind = "pcgamingwiki" | "steam" | "protondb" | "thunderstore";

export interface GameLink {
  kind: GameLinkKind;
  label: string;
  url: string;
}

export interface GameLinkSources {
  steamAppId: number | null;
  // The Thunderstore community identifier of a known moddable game.
  thunderstoreCommunity: string | null;
  linux: boolean;
}

const communityPattern = /^[a-z0-9][a-z0-9_-]{0,63}$/i;

function numericAppId(value: number | null): string | null {
  return value !== null && Number.isSafeInteger(value) && value > 0 ? String(value) : null;
}

// Only addresses built from the numeric App ID or a known community identifier, never from free text.
export function gameLinks({ steamAppId, thunderstoreCommunity, linux }: GameLinkSources): GameLink[] {
  const id = numericAppId(steamAppId);
  const links: GameLink[] = [];
  if (id !== null) {
    links.push({ kind: "pcgamingwiki", label: "PCGamingWiki", url: `https://www.pcgamingwiki.com/api/appid.php?appid=${id}` });
    links.push({ kind: "steam", label: "Steam", url: `https://store.steampowered.com/app/${id}` });
    if (linux) links.push({ kind: "protondb", label: "ProtonDB", url: `https://www.protondb.com/app/${id}` });
  }
  if (thunderstoreCommunity !== null && communityPattern.test(thunderstoreCommunity)) {
    links.push({ kind: "thunderstore", label: "Thunderstore", url: `https://thunderstore.io/c/${thunderstoreCommunity}/` });
  }
  return links;
}
