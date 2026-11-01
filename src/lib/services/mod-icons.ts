import { getModIcon } from "./modding";

interface CachedIcon {
  promise: Promise<string>;
  url: string | null;
  users: number;
}

const icons = new Map<string, CachedIcon>();
const maxIdleIcons = 96;

function prune(): void {
  const idle = [...icons].filter(([, entry]) => entry.users === 0 && entry.url !== null);
  for (const [key, entry] of idle.slice(0, Math.max(0, idle.length - maxIdleIcons))) {
    icons.delete(key);
    if (entry.url !== null) URL.revokeObjectURL(entry.url);
  }
}

// Object URLs are shared by every row showing the same icon and revoked once enough idle ones pile up.
export function acquireModIcon(source: string): { url: string | null; ready: Promise<string>; release: () => void } {
  let entry = icons.get(source);
  if (entry === undefined) {
    const created: CachedIcon = { promise: Promise.resolve(""), url: null, users: 0 };
    created.promise = getModIcon(source).then((icon) => {
      created.url = URL.createObjectURL(new Blob([icon.bytes], { type: icon.contentType }));
      return created.url;
    }).catch((error: unknown) => {
      if (icons.get(source) === created) icons.delete(source);
      throw error;
    }).finally(prune);
    icons.set(source, created);
    entry = created;
  }
  const acquired = entry;
  acquired.users++;
  let released = false;
  return {
    url: acquired.url,
    ready: acquired.promise,
    release: () => {
      if (released) return;
      released = true;
      acquired.users--;
      prune();
    },
  };
}
