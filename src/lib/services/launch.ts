import { invoke } from "./invoke";

export function launchConfiguredGameWithRunner(gameId: string): Promise<void> {
  return invoke<void>("launch_configured_game_with_runner", { gameId });
}

export function launchNativeGame(gameId: string): Promise<void> {
  return invoke<void>("launch_native_game", { gameId });
}
