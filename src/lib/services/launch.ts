import { invoke } from "./invoke";

// `last_used` starts the profile the game last launched with, `base` its base settings.
export type LaunchProfileChoice = { kind: "last_used" } | { kind: "base" } | { kind: "profile"; id: string };

// The backend picks Steam, a compatibility runner or a native launch for the game.
export function launchGame(gameId: string, profile: LaunchProfileChoice, confirmAccountSwitch: boolean): Promise<void> {
  return invoke<void>("launch_game", { gameId, profile, confirmAccountSwitch });
}
