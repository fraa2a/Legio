import { invoke } from "./invoke";

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  trayAvailable: boolean;
  desktopEnvironment: string | null;
  startupLaunchGameId: string | null;
  startupLaunchError: string | null;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}
