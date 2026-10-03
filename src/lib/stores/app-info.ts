import { getAppInfo, type AppInfo } from "../services/app";
import { createResource } from "./resource";

export const appInfo = createResource<AppInfo>(
  { name: "Legio", version: "", platform: "", trayAvailable: false, desktopEnvironment: null,
    startupLaunchGameId: null, startupLaunchError: null },
  getAppInfo,
);
