import { derived, readable, writable } from "svelte/store";
import {
  getQueueShutdown,
  listenQueueShutdownChanged,
  setQueueShutdown,
  type QueueShutdownStatus,
} from "../services/queue-shutdown";

interface Snapshot {
  status: QueueShutdownStatus;
  deadline: number | null;
}

function snapshot(status: QueueShutdownStatus): Snapshot {
  return {
    status,
    deadline: status.remainingSeconds === null ? null : Date.now() + status.remainingSeconds * 1000,
  };
}

const state = writable<Snapshot>(snapshot({ phase: "off", remainingSeconds: null, error: null }));

const clock = readable(Date.now(), (set) => {
  const timer = setInterval(() => set(Date.now()), 500);
  return () => clearInterval(timer);
});

export const queueShutdown = derived(state, (value) => value.status);

export const queueShutdownCountdown = derived([state, clock], ([value, now]) =>
  value.deadline === null ? null : Math.max(0, Math.ceil((value.deadline - now) / 1000)),
);

export async function startQueueShutdown(): Promise<void> {
  state.set(snapshot(await getQueueShutdown()));
  await listenQueueShutdownChanged((status) => state.set(snapshot(status)));
}

export async function enableQueueShutdown(enabled: boolean): Promise<void> {
  state.set(snapshot(await setQueueShutdown(enabled)));
}
