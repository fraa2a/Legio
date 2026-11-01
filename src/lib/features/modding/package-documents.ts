import { getModPackageDocument, type PackageDocument } from "../../services/modding";

const cacheLimit = 24;
const cache = new Map<string, Promise<string | null>>();

// Readmes are fetched once per version while the app runs; failures are not kept.
export function loadPackageDocument(fullName: string, version: string, document: PackageDocument): Promise<string | null> {
  const key = `${fullName.toLowerCase()}@${version}:${document}`;
  let pending = cache.get(key);
  if (pending === undefined) {
    pending = getModPackageDocument(fullName, version, document);
    pending.catch(() => cache.delete(key));
    cache.set(key, pending);
    // Maps keep insertion order, so the first key is the oldest request.
    const oldest = cache.keys().next();
    if (cache.size > cacheLimit && oldest.done !== true) cache.delete(oldest.value);
  }
  return pending;
}
