/**
 * Runs `run` for the newest value only: values pushed while a run is in flight
 * replace each other, and the last one runs when the current run ends.
 */
export function createLatestRunner<T>(run: (value: T) => Promise<unknown>) {
  let running = false;
  let next: { value: T } | null = null;

  const pump = async () => {
    running = true;
    while (next) {
      const { value } = next;
      next = null;
      try {
        await run(value);
      } catch {
        // A failed run must not stop later ones.
      }
    }
    running = false;
  };

  return {
    push(value: T) {
      next = { value };
      if (!running) void pump();
    },
  };
}
