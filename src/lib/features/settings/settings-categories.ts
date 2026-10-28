import type { IconName } from "../../components/ui/Icon.svelte";

export interface SettingsCategory {
  id: string;
  label: string;
  icon: IconName;
}

// Labels are translation keys; Compatibility only exists on Linux.
export function settingsCategories(platform: string): SettingsCategory[] {
  return [
    { id: "general", label: "Generali", icon: "settings" },
    { id: "appearance", label: "Aspetto", icon: "palette" },
    { id: "downloads", label: "Download", icon: "downloads" },
    { id: "libraries", label: "Librerie e sorgenti", icon: "folder" },
    ...(platform === "linux" ? [{ id: "compatibility", label: "Compatibilità", icon: "wrench" } as const] : []),
    { id: "advanced", label: "Avanzate", icon: "info" },
  ];
}
