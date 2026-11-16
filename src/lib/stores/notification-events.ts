import { t } from "../i18n";
import { updateState } from "../services/app-updater";
import { toMessage } from "../utils/errors";
import { downloads, retryJob } from "./downloads";
import { dropNotificationRetry, recordNotification } from "./notifications";
import { refreshSource, sourceRefreshError } from "./source";
import { showToast } from "./toast";

// Records background outcomes that otherwise only show up on their own page or as a system notification.
export function startNotificationEvents(): () => void {
  // Null until the first loaded list, so outcomes from before the session are not replayed.
  let known: Map<string, string> | null = null;
  const failedDownloads = new Map<string, number>();
  const stopDownloads = downloads.subscribe((state) => {
    if (state.status !== "ready" && state.status !== "empty") return;
    const next = new Map(state.data.map((job) => [job.id, job.status]));
    for (const job of known === null ? [] : state.data) {
      const before = known?.get(job.id);
      if (before === undefined || before === job.status) continue;
      if (job.status === "installed") {
        recordNotification(t("{0} installato.", undefined, [job.name]), "success");
      } else if (job.status === "failed") {
        const message = job.error === null
          ? t("Download di {0} non riuscito.", undefined, [job.name])
          : t("Download di {0} non riuscito: {1}", undefined, [job.name, job.error]);
        failedDownloads.set(job.id, recordNotification(message, "error", {
          run: () => void retryJob(job.id).catch((error: unknown) => showToast(toMessage(error), "error")),
        }));
      }
    }
    for (const [jobId, entryId] of failedDownloads) {
      if (next.get(jobId) === "failed") continue;
      dropNotificationRetry(entryId);
      failedDownloads.delete(jobId);
    }
    known = next;
  });

  let announcedVersion: string | null = null;
  const stopUpdates = updateState.subscribe(({ version }) => {
    if (version === null || version === announcedVersion) return;
    announcedVersion = version;
    recordNotification(t("Legio {0} disponibile.", undefined, [version]), "info");
  });

  let sourceFailure: number | null = null;
  const stopSource = sourceRefreshError.subscribe((error) => {
    if (sourceFailure !== null) dropNotificationRetry(sourceFailure);
    sourceFailure = error === null
      ? null
      : recordNotification(t("Aggiornamento delle sorgenti non riuscito: {0}", undefined, [error]), "error", { run: () => void refreshSource() });
  });

  return () => {
    stopDownloads();
    stopUpdates();
    stopSource();
  };
}
