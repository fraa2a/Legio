import { invoke } from "./invoke";
import { decodeImageResponse } from "./image-response";

export interface GameScreenshot {
  id: string;
  name: string;
  source: "steam" | "folder";
  modifiedMs: number;
}

export interface GameScreenshotPage {
  entries: GameScreenshot[];
  total: number;
  pageSize: number;
  folder: string;
  customFolder: boolean;
}

export const listGameScreenshots = (gameId: string, offset: number) =>
  invoke<GameScreenshotPage>("list_game_screenshots", { gameId, offset });

// The caller owns the returned object URL and must revoke it.
export async function loadGameScreenshotUrl(gameId: string, screenshotId: string, thumbnail: boolean): Promise<string> {
  const payload = await invoke<ArrayBuffer>("get_game_screenshot", { gameId, screenshotId, thumbnail });
  const image = decodeImageResponse<{ contentType: string }>(payload);
  return URL.createObjectURL(new Blob([image.bytes], { type: image.contentType }));
}

export const openGameScreenshotsFolder = (gameId: string, screenshotId: string | null) =>
  invoke<void>("open_game_screenshots_folder", { gameId, screenshotId });

export const setGameScreenshotsFolder = (gameId: string, folder: string | null) =>
  invoke<void>("set_game_screenshots_folder", { gameId, folder });

export const importGameScreenshotBackground = (gameId: string, screenshotId: string) =>
  invoke<string>("import_game_screenshot_background", { gameId, screenshotId });
