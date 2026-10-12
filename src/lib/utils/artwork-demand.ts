import { getContext, setContext } from "svelte";

const key = Symbol("artwork-demand");
export function artworkDemand(): () => boolean {
  return getContext<() => boolean>(key) ?? (() => true);
}
export function provideArtworkDemand(active: () => boolean): void {
  setContext(key, active);
}
