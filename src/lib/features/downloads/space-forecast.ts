import { t, type Language } from "../../i18n";
import type { SpaceForecast, VolumeSpace } from "../../services/downloads";
import { formatBytes } from "../../utils/format";

export interface SpaceWarning {
  tone: "primary" | "danger";
  title: string;
  message: string;
  confirmLabel: string;
  details: string[];
}

function describeVolume(volume: VolumeSpace, sizeKnown: boolean, selected?: Language): string {
  if (volume.freeBytes === null) return t("{0}: spazio libero non disponibile", selected, [volume.path]);
  const free = formatBytes(volume.freeBytes, selected);
  const line = sizeKnown
    ? t("{0}: servono da {1} a {2}, liberi {3}", selected, [volume.path, formatBytes(volume.minimumBytes, selected), formatBytes(volume.estimatedBytes, selected), free])
    : t("{0}: liberi {1}", selected, [volume.path, free]);
  return volume.reservedBytes > 0
    ? `${line} ${t("({0} già riservati ad altri download)", selected, [formatBytes(volume.reservedBytes, selected)])}`
    : line;
}

/** The warning to confirm before queueing, or null when the space is clearly enough. */
export function spaceWarning(forecast: SpaceForecast, selected?: Language): SpaceWarning | null {
  if (forecast.verdict === "enough") return null;
  const sizeKnown = forecast.sizeBytes !== null;
  const details = forecast.volumes.map(volume => describeVolume(volume, sizeKnown, selected));
  if (!sizeKnown) details.unshift(t("La dimensione del download non è nota.", selected));
  if (forecast.verdict === "insufficient") {
    return {
      tone: "danger",
      title: t("Spazio su disco insufficiente", selected),
      message: t("Lo spazio libero non basta per l'archivio e i file estratti: il download fallirebbe prima dell'installazione. Libera spazio o scegli un'altra libreria.", selected),
      confirmLabel: t("Ignora l'avviso e scarica", selected),
      details,
    };
  }
  return {
    tone: "primary",
    title: forecast.verdict === "tight" ? t("Spazio su disco limitato", selected) : t("Spazio su disco non verificabile", selected),
    message: forecast.verdict === "tight"
      ? t("Lo spazio libero potrebbe non bastare per l'archivio e i file estratti, stimati fino al doppio dell'archivio.", selected)
      : t("Non è possibile verificare se lo spazio libero basta per questo download.", selected),
    confirmLabel: t("Scarica comunque", selected),
    details,
  };
}
