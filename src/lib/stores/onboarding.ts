import { get, writable } from "svelte/store";
import { setLanguage } from "../i18n";
import { type Settings } from "../services/local-state";
import { pickGameDirectory } from "../services/dialog";
import { getCompatibilityDefaults, listCompatibilityRunners, type CompatibilityDefaults, type CompatibilityRunner } from "../services/game-settings";
import { saveOnboarding } from "../services/onboarding";
import { settings } from "./settings";
import { appearancePreview } from "./appearance";
import { activeSection } from "./navigation";
import { appInfo } from "./app-info";
import { toMessage } from "../utils/errors";

interface State {
  draft: Settings | null;
  compatibility: CompatibilityDefaults | null;
  runners: CompatibilityRunner[];
  diagnostics: string[];
  loading: boolean;
  saving: boolean;
  error: string | null;
}
export const onboarding = writable<State>({ draft: null, compatibility: null, runners: [], diagnostics: [], loading: false, saving: false, error: null });
let loadId = 0;

export async function startOnboarding(): Promise<void> {
  const request = ++loadId;
  const draft = structuredClone(get(settings).data);
  onboarding.set({ draft, compatibility: null, runners: [], diagnostics: [], loading: get(appInfo).data.platform === "linux", saving: false, error: null });
  if (get(appInfo).data.platform !== "linux") return;
  const [defaults, discovery] = await Promise.allSettled([getCompatibilityDefaults(), listCompatibilityRunners()]);
  if (request !== loadId) return;
  onboarding.update((state) => ({ ...state, loading: false,
    compatibility: defaults.status === "fulfilled" ? defaults.value : null,
    runners: discovery.status === "fulfilled" ? discovery.value.runners : [],
    diagnostics: discovery.status === "fulfilled" ? discovery.value.diagnostics : [toMessage(discovery.reason)],
    error: defaults.status === "rejected" ? toMessage(defaults.reason) : null,
  }));
}

export function changeOnboarding(changes: Partial<Settings>): void {
  onboarding.update((state) => {
    if (!state.draft) return state;
    const draft = { ...state.draft, ...changes };
    setLanguage(draft.language);
    appearancePreview.set({ theme: draft.theme, appearance: draft.appearance });
    return { ...state, draft, error: null };
  });
}

export function changeCompatibility(changes: Partial<CompatibilityDefaults>): void {
  onboarding.update((state) => ({ ...state, compatibility: state.compatibility ? { ...state.compatibility, ...changes } : null, error: null }));
}

export async function chooseOnboardingDirectory(prefix = false): Promise<void> {
  try {
    const state = get(onboarding);
    const path = await pickGameDirectory(prefix ? state.compatibility?.prefixRoot : state.draft?.downloadPath);
    if (path === null) return;
    if (prefix) changeCompatibility({ prefixRoot: path });
    else changeOnboarding({ downloadPath: path });
  } catch (error) { onboarding.update((state) => ({ ...state, error: toMessage(error) })); }
}

export async function completeOnboarding(): Promise<void> {
  const state = get(onboarding);
  if (!state.draft || state.saving || state.loading) return;
  onboarding.update((state) => ({ ...state, saving: true, error: null }));
  try {
    const saved = await saveOnboarding(state.draft, state.compatibility);
    activeSection.set(saved.launchInLibrary ? "library" : "home");
    settings.set(saved);
    setLanguage(saved.language);
    appearancePreview.set(null);
  } catch (error) { onboarding.update((state) => ({ ...state, error: toMessage(error) })); }
  finally { onboarding.update((state) => ({ ...state, saving: false })); }
}

export function endOnboardingPreview(): void {
  ++loadId;
  setLanguage(get(settings).data.language);
  appearancePreview.set(null);
}
