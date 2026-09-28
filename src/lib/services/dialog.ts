import { open } from "@tauri-apps/plugin-dialog";

function defaultPath(value: string | null | undefined): string | undefined {
  return value !== null && value !== undefined && value.length > 0 ? value : undefined;
}

export function pickGameDirectory(startPath?: string | null, title = "Seleziona la cartella del gioco"): Promise<string | null> {
  return open({
    title,
    directory: true,
    multiple: false,
    defaultPath: defaultPath(startPath),
  });
}

export function pickExecutableFile(startPath?: string | null): Promise<string | null> {
  return open({
    title: "Seleziona l'eseguibile del gioco",
    directory: false,
    multiple: false,
    defaultPath: defaultPath(startPath),
  });
}

export function pickGameArtworkFile(): Promise<string | null> {
  return open({
    title: "Seleziona un'immagine",
    directory: false,
    multiple: false,
    filters: [{ name: "Immagini", extensions: ["png", "jpg", "jpeg", "webp"] }],
  });
}
