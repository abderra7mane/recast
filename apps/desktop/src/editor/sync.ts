export type Timers = {
  setTimeout: (fn: () => void, ms: number) => unknown;
  clearTimeout: (id: unknown) => void;
};

const defaultTimers: Timers = {
  setTimeout: (fn, ms) => globalThis.setTimeout(fn, ms),
  clearTimeout: (id) => globalThis.clearTimeout(id as number),
};

/**
 * Sends values to the backend at most once per `delayMs` while they keep
 * changing (the last value always goes out), or right away on `commit`.
 */
export function createSettingsSync<T>(
  send: (value: T) => void,
  delayMs = 100,
  timers: Timers = defaultTimers,
) {
  let pending: { value: T } | null = null;
  let timer: unknown = null;

  const fire = () => {
    timer = null;
    if (!pending) return;
    const { value } = pending;
    pending = null;
    send(value);
  };

  return {
    schedule(value: T) {
      pending = { value };
      if (timer === null) timer = timers.setTimeout(fire, delayMs);
    },
    commit(value: T) {
      if (timer !== null) timers.clearTimeout(timer);
      timer = null;
      pending = null;
      send(value);
    },
    /** Sends a scheduled value now, if there is one. */
    flush() {
      if (timer !== null) timers.clearTimeout(timer);
      fire();
    },
    cancel() {
      if (timer !== null) timers.clearTimeout(timer);
      timer = null;
      pending = null;
    },
  };
}

/**
 * Numbers requests and accepts only results newer than the newest one accepted,
 * since results can come back in a different order than the requests went out.
 */
export function createSequence() {
  let sent = 0;
  let newest = 0;
  return {
    next: () => ++sent,
    accept(seq: number) {
      if (seq <= newest) return false;
      newest = seq;
      return true;
    },
  };
}
