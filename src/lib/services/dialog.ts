import { t } from "../i18n";
import { open, save } from "@tauri-apps/plugin-dialog";

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

export function pickSourceFile(): Promise<string | null> {
  return open({
    title: t("Seleziona una sorgente JSON", undefined),
    directory: false,
    multiple: false,
    filters: [{ name: "JSON (Legio / Hydra)", extensions: ["json"] }],
  });
}

export function pickModArchive(): Promise<string | null> {
  return open({
    title: t("Seleziona l'archivio della mod", undefined),
    directory: false,
    multiple: false,
    filters: [{ name: t("Archivi", undefined), extensions: ["zip", "7z", "rar"] }],
  });
}

const modProfileFilters = () => [{ name: t("Profilo mod (r2modman)", undefined), extensions: ["r2z"] }];

export function pickModProfileFile(): Promise<string | null> {
  return open({ title: t("Seleziona un profilo da importare", undefined), directory: false, multiple: false, filters: modProfileFilters() });
}

export function pickModProfileExportPath(defaultName: string): Promise<string | null> {
  return save({ title: t("Esporta profilo", undefined), defaultPath: defaultName, filters: modProfileFilters() });
}
