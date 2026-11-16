export const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

// Runs `check` on a fixed interval until stopped. A tick is skipped while `isBusy` is true
// or the previous check has not finished.
export function startPeriodicChecks(check: () => Promise<void>, isBusy: () => boolean, intervalMs = UPDATE_CHECK_INTERVAL_MS): () => void {
  let running = false;
  const timer = setInterval(() => {
    if (running || isBusy()) return;
    running = true;
    void check().finally(() => { running = false; });
  }, intervalMs);
  return () => clearInterval(timer);
}
