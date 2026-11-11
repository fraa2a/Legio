import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export function onDownloadListChanged(callback: () => void): Promise<UnlistenFn> {
  return listen("legio:downloads-changed", () => callback());
}
