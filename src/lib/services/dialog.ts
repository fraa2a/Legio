import { t } from "../i18n";
import { open } from "@tauri-apps/plugin-dialog";

function defaultPath(value: string | null | undefined): string | undefined {
  return value !== null && value !== undefined && value.length > 0 ? value : undefined;
}

export function pickGameDirectory(startPath?: string | null, title = t("Seleziona la cartella del gioco", undefined)): Promise<string | null> {
  return open({
    title,
    directory: true,
    multiple: false,
    defaultPath: defaultPath(startPath),
  });
}

export function pickExecutableFile(startPath?: string | null): Promise<string | null> {
  return open({
    title: t("Seleziona l'eseguibile del gioco", undefined),
    directory: false,
    multiple: false,
    defaultPath: defaultPath(startPath),
  });
}

export function pickGameArtworkFile(): Promise<string | null> {
  return open({
    title: t("Seleziona un'immagine", undefined),
    directory: false,
    multiple: false,
    filters: [{ name: t("Immagini", undefined), extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
}
