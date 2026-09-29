import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

export function minimizeWindow(): Promise<void> {
  return appWindow.minimize();
}

export function closeWindow(): Promise<void> {
  return appWindow.close();
}

export function hideWindow(): Promise<void> {
  return appWindow.hide();
}

export async function showWindow(): Promise<void> {
  await appWindow.show();
  await appWindow.setFocus();
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
