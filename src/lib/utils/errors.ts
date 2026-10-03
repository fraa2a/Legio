import { t } from "../i18n";
export function toMessage(error: unknown): string {
  if (typeof error === "string") return t(error);
  if (error instanceof Error) return t(error.message);
  if (error !== null && typeof error === "object" && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return t(message);
  }
  return t("Errore sconosciuto", undefined);
}
