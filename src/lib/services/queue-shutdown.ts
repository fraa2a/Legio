import { listen } from "@tauri-apps/api/event";
import { invoke } from "./invoke";

export interface QueueShutdownStatus {
  phase: "off" | "waiting" | "countdown";
  remainingSeconds: number | null;
  error: string | null;
}

export const getQueueShutdown = () => invoke<QueueShutdownStatus>("get_queue_shutdown");
export const setQueueShutdown = (enabled: boolean) => invoke<QueueShutdownStatus>("set_queue_shutdown", { enabled });
export const listenQueueShutdownChanged = (callback: (status: QueueShutdownStatus) => void) =>
  listen<QueueShutdownStatus>("queue-shutdown-changed", (event) => callback(event.payload));
