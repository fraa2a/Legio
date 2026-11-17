import { get } from "svelte/store";
import { createResource } from "./resource";
import { getChangelog, refreshChangelog, type ChangelogSnapshot } from "../services/changelog";
import { toMessage } from "../utils/errors";

export const changelog = createResource<ChangelogSnapshot>({ releases: [], cachedAt: null, warning: null }, getChangelog);
let pending: Promise<void> | null = null;
export async function loadChangelog(force = false): Promise<void> {
  if (pending) return pending;
  pending = (async () => {
    if (get(changelog).status === "idle") await changelog.load();
    const snapshot = get(changelog).data;
    const age = snapshot.cachedAt === null ? null : Date.now() / 1000 - snapshot.cachedAt;
    if (!force && age !== null && age >= 0 && age < 60 * 60) return;
    try { changelog.set(await refreshChangelog()); }
    catch (error) { changelog.set({ ...get(changelog).data, warning: toMessage(error) }); }
  })().finally(() => { pending = null; });
  return pending;
}
