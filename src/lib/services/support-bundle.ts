import { invoke } from "./invoke";
import { save } from "@tauri-apps/plugin-dialog";
import { t } from "../i18n";

// Returns false when the user cancels the save dialog.
export async function createSupportBundle(): Promise<boolean> {
  const path = await save({ title: t("Crea pacchetto di supporto"), defaultPath: "legio-support.zip", filters: [{ name: "ZIP", extensions: ["zip"] }] });
  if (path === null) return false;
  await invoke("create_support_bundle", { path });
  return true;
}
