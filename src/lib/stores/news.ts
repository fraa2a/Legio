import { get } from "svelte/store";
import { createResource } from "./resource";
import { getNews, refreshNews, type NewsSnapshot } from "../services/news";
import { toMessage } from "../utils/errors";

export const news = createResource<NewsSnapshot>({ feed: null, cachedAt: null, warning: null }, getNews);
let pending: Promise<void> | null = null;
export async function loadNews(force = false): Promise<void> {
  if (pending) return pending;
  pending = (async () => {
    if (get(news).status === "idle") await news.load();
    const snapshot = get(news).data;
    if (!force && snapshot.cachedAt !== null && Date.now() / 1000 - snapshot.cachedAt < 15 * 60) return;
    try { news.set(await refreshNews()); }
    catch (error) { news.set({ ...get(news).data, warning: toMessage(error) }); }
  })().finally(() => { pending = null; });
  return pending;
}
