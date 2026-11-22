import { invoke } from "./invoke";
import type { Language } from "../i18n";

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  trayAvailable: boolean;
  desktopEnvironment: string | null;
  systemLanguage: Language;
  startupLaunchGameId: string | null;
  startupLaunchError: string | null;
  startupWarnings: string[];
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("get_app_info");
}
