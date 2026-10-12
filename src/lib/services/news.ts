import { invoke } from "./invoke";
export interface NewsText { it: string; en: string }
export interface NewsArticle { id: string; publishedAt: string; title: NewsText; summary: NewsText; body: NewsText }
export interface NewsFeed { schemaVersion: 1; generatedAt: string; items: NewsArticle[] }
export interface NewsSnapshot { feed: NewsFeed | null; cachedAt: number | null; warning: string | null }
export function getNews(): Promise<NewsSnapshot> { return invoke("get_news"); }
export function refreshNews(): Promise<NewsSnapshot> { return invoke("refresh_news"); }
