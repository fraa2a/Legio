import type { ModManagerSelection, Section, StoreGameSelection } from "../../stores/navigation";

export interface TitleBarLocation {
  section: Section;
  gameId: string | null;
  storeGame: StoreGameSelection | null;
  modGame: ModManagerSelection | null;
  // The play recap is a page inside Home.
  recapOpen: boolean;
}

// The path ends with the current page; a leading `back` crumb names the page that back navigation returns to.
export interface TitleBarPath {
  back: string | null;
  current: string;
}

export function titleBarPath(
  location: TitleBarLocation,
  gameName: (id: string) => string | null,
  sectionLabel: (section: Section) => string,
  recapLabel: string,
): TitleBarPath {
  const { section } = location;
  if (section === "home" && location.recapOpen) {
    return { back: sectionLabel("home"), current: recapLabel };
  }
  if (section === "store" && location.storeGame !== null) {
    return { back: sectionLabel("store"), current: location.storeGame.name };
  }
  if (section === "mods" && location.modGame !== null) {
    const name = gameName(location.modGame.gameId);
    return location.modGame.returnTo === "game"
      ? { back: name ?? sectionLabel("library"), current: sectionLabel("mods") }
      : { back: sectionLabel("mods"), current: name ?? sectionLabel("mods") };
  }
  if (section === "library" && location.gameId !== null) {
    // An unknown game keeps the plain section, as the page has nothing to show either.
    const name = gameName(location.gameId);
    if (name !== null) return { back: sectionLabel("library"), current: name };
  }
  return { back: null, current: sectionLabel(section) };
}

export interface DownloadSummary {
  count: number;
  percent: number;
  speedBps: number;
}

// Progress is weighted by size, so a large download moves the total more than a small one.
export function summarizeDownloads(jobs: readonly { sizeBytes: number; downloadedBytes: number; speedBps: number }[]): DownloadSummary | null {
  if (jobs.length === 0) return null;
  let size = 0;
  let done = 0;
  let speedBps = 0;
  for (const job of jobs) {
    if (Number.isFinite(job.sizeBytes) && job.sizeBytes > 0) {
      size += job.sizeBytes;
      done += Number.isFinite(job.downloadedBytes) ? Math.min(job.sizeBytes, Math.max(0, job.downloadedBytes)) : 0;
    }
    if (Number.isFinite(job.speedBps) && job.speedBps > 0) speedBps += job.speedBps;
  }
  return { count: jobs.length, percent: size === 0 ? 0 : Math.floor((done / size) * 100), speedBps };
}
