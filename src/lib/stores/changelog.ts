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
    try { changelog.set(await refreshChangelog(force)); }
    catch (error) { changelog.set({ ...get(changelog).data, warning: toMessage(error) }); }
  })().finally(() => { pending = null; });
  return pending;
}
