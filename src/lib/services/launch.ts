import { invoke } from "./invoke";

// A null profile launches with the game's base settings.
export function launchConfiguredGameWithRunner(gameId: string, profileId: string | null): Promise<void> {
  return invoke<void>("launch_configured_game_with_runner", { gameId, profileId });
}

export function launchNativeGame(gameId: string, profileId: string | null): Promise<void> {
  return invoke<void>("launch_native_game", { gameId, profileId });
}
