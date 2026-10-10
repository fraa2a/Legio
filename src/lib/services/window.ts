import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();
const inactiveListeners = new Set<() => void>();

export async function minimizeWindow(): Promise<void> {
  await appWindow.minimize();
  for (const listener of inactiveListeners) listener();
}

export function closeWindow(): Promise<void> {
  return appWindow.close();
}

export async function hideWindow(): Promise<void> {
  await appWindow.hide();
  for (const listener of inactiveListeners) listener();
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

export function onWindowActivity(
  listener: (active: boolean) => void,
  { requireFocus = true }: { requireFocus?: boolean } = {},
): () => void {
  let disposed = false;
  let revision = 0;
  const unlisten: (() => void)[] = [];
  const inactive = () => {
    revision++;
    listener(false);
  };
  const refresh = () => {
    const requested = ++revision;
    if (document.hidden) {
      listener(false);
      return;
    }
    void Promise.all([requireFocus ? appWindow.isFocused() : Promise.resolve(true), appWindow.isVisible(), appWindow.isMinimized()])
      .then(([focused, visible, minimized]) => {
        if (!disposed && requested === revision) listener(focused && visible && !minimized && !document.hidden);
      }).catch((error: unknown) => {
        if (!disposed && requested === revision) {
          listener(false);
          console.warn("Could not inspect window activity", error);
        }
      });
  };
  for (const subscription of [
    appWindow.onFocusChanged(({ payload }) => payload || !requireFocus ? refresh() : inactive()),
    appWindow.onResized(refresh),
    appWindow.listen("legio:window-hidden", inactive),
  ]) {
    void subscription.then((stop) => disposed ? stop() : unlisten.push(stop))
      .catch((error: unknown) => console.warn("Could not observe window activity", error));
  }
  const blurred = requireFocus ? inactive : refresh;
  inactiveListeners.add(inactive);
  document.addEventListener("visibilitychange", refresh);
  window.addEventListener("blur", blurred);
  window.addEventListener("focus", refresh);
  refresh();
  return () => {
    disposed = true;
    revision++;
    inactiveListeners.delete(inactive);
    for (const stop of unlisten) stop();
    document.removeEventListener("visibilitychange", refresh);
    window.removeEventListener("blur", blurred);
    window.removeEventListener("focus", refresh);
  };
}
