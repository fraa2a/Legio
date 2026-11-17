import { invoke } from "./invoke";
export interface ChangelogRelease { version: string; publishedAt: string; notes: string; url: string; prerelease: boolean }
export interface ChangelogSnapshot { releases: ChangelogRelease[]; cachedAt: number | null; warning: string | null }
export function getChangelog(): Promise<ChangelogSnapshot> { return invoke("get_changelog"); }
export function refreshChangelog(): Promise<ChangelogSnapshot> { return invoke("refresh_changelog"); }
