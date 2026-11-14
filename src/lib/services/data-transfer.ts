import { invoke } from "./invoke";

export const exportUserData = (destinationPath: string) => invoke<void>("export_user_data", { destinationPath });
export const stageUserDataImport = (sourcePath: string) => invoke<void>("stage_user_data_import", { sourcePath });
