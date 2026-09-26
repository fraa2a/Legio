import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

export function minimizeWindow(): Promise<void> {
  return appWindow.minimize();
}

export function closeWindow(): Promise<void> {
  return appWindow.close();
}

export function toggleMaximizeWindow(): Promise<void> {
  return appWindow.toggleMaximize();
}

export function isWindowMaximized(): Promise<boolean> {
  return appWindow.isMaximized();
}

export function onWindowResized(listener: () => void): Promise<() => void> {
  return appWindow.onResized(listener);
}