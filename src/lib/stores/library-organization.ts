import { derived, get } from "svelte/store";
import {
  createCollection,
  deleteCollection,
  getLibraryOrganization,
  renameCollection,
  reorderCollections,
  setGamesCollections,
  setGamesFavorite,
  setGamesPlayStatus,
  type GameOrganization,
  type LibraryOrganization,
  type PlayStatus,
} from "../services/library-organization";
import { createResource } from "./resource";
import { libraryView, updateLibraryView } from "./library-ui";

export const libraryOrganization = createResource<LibraryOrganization>({ games: [], collections: [] }, getLibraryOrganization);

export const organizationByGame = derived(
  libraryOrganization,
  ($organization) => new Map<string, GameOrganization>($organization.data.games.map((game) => [game.gameId, game])),
);

async function apply(task: Promise<LibraryOrganization>): Promise<void> {
  libraryOrganization.set(await task);
}

export function setFavorite(gameIds: string[], favorite: boolean): Promise<void> {
  return apply(setGamesFavorite(gameIds, favorite));
}

export function setPlayStatus(gameIds: string[], status: PlayStatus): Promise<void> {
  return apply(setGamesPlayStatus(gameIds, status));
}

export function setCollections(gameIds: string[], collectionIds: string[]): Promise<void> {
  return apply(setGamesCollections(gameIds, collectionIds));
}

/** Creates a collection and returns its ID. */
export async function addCollection(name: string): Promise<string> {
  const before = new Set(get(libraryOrganization).data.collections.map((collection) => collection.id));
  const next = await createCollection(name);
  libraryOrganization.set(next);
  const created = next.collections.find((collection) => !before.has(collection.id));
  if (created === undefined) throw new Error("The new collection was not returned");
  return created.id;
}

export function renameExistingCollection(id: string, name: string): Promise<void> {
  return apply(renameCollection(id, name));
}

export async function removeCollection(id: string): Promise<void> {
  await apply(deleteCollection(id));
  const filter = get(libraryView).collection;
  if (filter.kind === "collection" && filter.id === id) await updateLibraryView({ collection: { kind: "all" } });
}

/** Moves a collection one step up (-1) or down (1). */
export function moveCollection(id: string, step: -1 | 1): Promise<void> {
  const ids = get(libraryOrganization).data.collections.map((collection) => collection.id);
  const from = ids.indexOf(id);
  const to = from + step;
  if (from < 0 || to < 0 || to >= ids.length) return Promise.resolve();
  [ids[from], ids[to]] = [ids[to], ids[from]];
  return apply(reorderCollections(ids));
}
