import { writable, type Readable } from "svelte/store";
import { toMessage } from "../utils/errors";

export type LoadStatus = "idle" | "loading" | "ready" | "empty" | "error";

interface LoadState<T> {
  status: LoadStatus;
  data: T;
  error: string | null;
}

type Resource<T> = Readable<LoadState<T>> & {
  load: () => Promise<void>;
  set: (data: T) => void;
};

export function createResource<T>(
  initial: T,
  loadValue: () => Promise<T>,
  isEmpty: (data: T) => boolean = () => false,
): Resource<T> {
  const state = writable<LoadState<T>>({ status: "idle", data: initial, error: null });
  let requestId = 0;

  async function load(): Promise<void> {
    const request = ++requestId;
    state.update((previous) => ({ status: "loading", data: previous.data, error: null }));
    try {
      const data = await loadValue();
      if (request !== requestId) return;
      state.set({ status: isEmpty(data) ? "empty" : "ready", data, error: null });
    } catch (error) {
      if (request !== requestId) return;
      state.update((previous) => ({ status: "error", data: previous.data, error: toMessage(error) }));
    }
  }

  function set(data: T): void {
    requestId += 1;
    state.set({ status: isEmpty(data) ? "empty" : "ready", data, error: null });
  }

  return { subscribe: state.subscribe, load, set };
}
