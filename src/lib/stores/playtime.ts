import { getPlaytimeSummaries, getRecentGameIds, type PlaytimeSummary } from "../services/playtime";
import { createResource } from "./resource";

export const recentGameIds = createResource<string[]>([], getRecentGameIds);

// Recent games follow the same sessions, so they refresh with the summaries.
export const playtime = createResource<PlaytimeSummary[]>([], () => {
  void recentGameIds.reload();
  return getPlaytimeSummaries();
});
