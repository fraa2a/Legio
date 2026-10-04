import { saveSettings, type Settings } from "./local-state";
import { saveCompatibilityDefaults, type CompatibilityDefaults } from "./game-settings";

export async function saveOnboarding(draft: Settings, compatibility: CompatibilityDefaults | null): Promise<Settings> {
  if (compatibility) await saveCompatibilityDefaults(compatibility);
  return saveSettings({ ...draft, onboardingComplete: true });
}
