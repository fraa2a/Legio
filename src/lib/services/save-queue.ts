export function createSaveQueue() {
  let pending: Promise<void> | null = null;

  function run(save: () => Promise<void>): Promise<void> {
    if (pending !== null) return pending;
    const task = (async () => { await save(); })().finally(() => {
      if (pending === task) pending = null;
    });
    pending = task;
    return task;
  }

  async function flush(saveLatest: () => Promise<void>): Promise<void> {
    if (pending !== null) await pending;
    await saveLatest();
  }

  return { run, flush };
}
