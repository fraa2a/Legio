import type { Game, LibraryCollectionFilter, LibrarySortKey } from "../../services/local-state";
import type { Collection, GameOrganization, PlayStatus } from "../../services/library-organization";
import type { PlaytimeSummary } from "../../services/playtime";

export type LibraryOrigin = "steam" | "local";

/** One library card: the shown version of a game and what its versions share. */
export interface LibraryEntry {
  game: Game;
  gameIds: string[];
  totalMilliseconds: number;
  lastPlayedAt: number | null;
  addedAt: number | null;
  favorite: boolean;
  playStatus: PlayStatus;
  collectionIds: ReadonlySet<string>;
  origin: LibraryOrigin;
}

export interface LibraryFilters {
  favoritesOnly: boolean;
  playStatus: PlayStatus | null;
  collection: LibraryCollectionFilter;
}

export interface LibraryShelf {
  // null holds the games outside every collection.
  id: string | null;
  name: string;
  entries: LibraryEntry[];
}

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });
const noCollections: ReadonlySet<string> = new Set();

export function collectionMembership(collections: Collection[]): Map<string, Set<string>> {
  const membership = new Map<string, Set<string>>();
  for (const collection of collections) {
    for (const gameId of collection.gameIds) {
      const set = membership.get(gameId);
      if (set === undefined) membership.set(gameId, new Set([collection.id]));
      else set.add(collection.id);
    }
  }
  return membership;
}

function latest(values: (number | null | undefined)[], pick: (a: number, b: number) => number): number | null {
  let result: number | null = null;
  for (const value of values) {
    if (value === null || value === undefined) continue;
    result = result === null ? value : pick(result, value);
  }
  return result;
}

/**
 * Versions of one game share a card, so organization is read across the group:
 * favorite and membership if any version has them, the status of the shown version
 * or else the first set one, and the earliest known addition.
 */
export function libraryEntries(
  items: { game: Game; group: Game[]; totalMilliseconds: number }[],
  summaries: Map<string, PlaytimeSummary>,
  organization: Map<string, GameOrganization>,
  membership: Map<string, Set<string>>,
): LibraryEntry[] {
  return items.map(({ game, group, totalMilliseconds }) => {
    const states = group.map((member) => organization.get(member.id));
    const shownStatus = organization.get(game.id)?.playStatus ?? "none";
    const collectionIds = group.length === 1
      ? (membership.get(game.id) ?? noCollections)
      : new Set(group.flatMap((member) => [...(membership.get(member.id) ?? [])]));
    return {
      game,
      gameIds: group.map((member) => member.id),
      totalMilliseconds,
      lastPlayedAt: latest(group.map((member) => summaries.get(member.id)?.lastPlayedAt), Math.max),
      addedAt: latest(states.map((state) => state?.addedAt), Math.min),
      favorite: states.some((state) => state?.favorite === true),
      playStatus: shownStatus !== "none" ? shownStatus : (states.find((state) => state !== undefined && state.playStatus !== "none")?.playStatus ?? "none"),
      collectionIds,
      origin: game.steamInstallPath !== null ? "steam" : "local",
    };
  });
}

export function filterEntries(entries: LibraryEntry[], filters: LibraryFilters, collections: Collection[]): LibraryEntry[] {
  const { collection } = filters;
  // A filter on a deleted collection shows everything rather than nothing.
  const collectionId = collection.kind === "collection" && collections.some((item) => item.id === collection.id) ? collection.id : null;
  return entries.filter((entry) =>
    (!filters.favoritesOnly || entry.favorite)
    && (filters.playStatus === null || entry.playStatus === filters.playStatus)
    && (collection.kind !== "uncollected" || entry.collectionIds.size === 0)
    && (collectionId === null || entry.collectionIds.has(collectionId)));
}

export function defaultDescending(key: LibrarySortKey): boolean {
  return key === "playtime" || key === "lastPlayed" || key === "added";
}

/** Choosing the current key flips the direction; a new key starts in its natural one. */
export function nextSort(current: { sort: LibrarySortKey; descending: boolean }, key: LibrarySortKey): { sort: LibrarySortKey; descending: boolean } {
  return current.sort === key ? { sort: key, descending: !current.descending } : { sort: key, descending: defaultDescending(key) };
}

function compareKey(left: LibraryEntry, right: LibraryEntry, key: LibrarySortKey): number | null {
  if (key === "playtime") return left.totalMilliseconds - right.totalMilliseconds;
  if (key === "origin") return left.origin === right.origin ? 0 : left.origin === "steam" ? -1 : 1;
  if (key === "name") return collator.compare(left.game.name, right.game.name);
  const a = key === "lastPlayed" ? left.lastPlayedAt : left.addedAt;
  const b = key === "lastPlayed" ? right.lastPlayedAt : right.addedAt;
  // Unknown dates stay last in both directions.
  if (a === null || b === null) return a === b ? 0 : null;
  return a - b;
}

export function sortEntries(entries: LibraryEntry[], sort: LibrarySortKey, descending: boolean): LibraryEntry[] {
  const direction = descending ? -1 : 1;
  return [...entries].sort((left, right) => {
    const compared = compareKey(left, right, sort);
    if (compared === null) return left[sort === "lastPlayed" ? "lastPlayedAt" : "addedAt"] === null ? 1 : -1;
    if (compared !== 0) return compared * direction;
    return collator.compare(left.game.name, right.game.name) || left.game.id.localeCompare(right.game.id);
  });
}

/** One shelf per collection in collection order, then the games outside every collection. */
export function libraryShelves(entries: LibraryEntry[], collections: Collection[], uncollectedName: string): LibraryShelf[] {
  const shelves: LibraryShelf[] = collections.map((collection) => ({
    id: collection.id,
    name: collection.name,
    entries: entries.filter((entry) => entry.collectionIds.has(collection.id)),
  }));
  shelves.push({ id: null, name: uncollectedName, entries: entries.filter((entry) => entry.collectionIds.size === 0) });
  return shelves.filter((shelf) => shelf.entries.length > 0);
}
