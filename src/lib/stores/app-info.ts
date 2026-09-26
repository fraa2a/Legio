import { getAppInfo, type AppInfo } from "../services/app";
import { createResource } from "./resource";

export const appInfo = createResource<AppInfo>(
  { name: "Legio", version: "", platform: "", desktopEnvironment: null },
  getAppInfo,
);
