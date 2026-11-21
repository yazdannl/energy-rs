/**
 * Repeating refreshes; each helper returns its own teardown for `onMount`.
 *
 * `pollVisible` is the shared, battery-friendly variant: it stops while the tab is hidden and does
 * one immediate refresh plus a fresh interval when the tab becomes visible again. Pages that show
 * live values should use it; slow bookkeeping can keep using `poll`.
 */

export interface VisibilityHost {
  readonly visibilityState: string;
  addEventListener(type: string, listener: () => void): void;
  removeEventListener(type: string, listener: () => void): void;
}

/** A repeating refresh; returns its own teardown so a page can use it from `onMount`. */
export function poll(run: () => void, everyMs = 300_000): () => void {
  const timer = setInterval(run, everyMs);
  return () => clearInterval(timer);
}

function browserHost(): VisibilityHost {
  if (typeof document === 'undefined') {
    return {
      visibilityState: 'visible',
      addEventListener() {},
      removeEventListener() {},
    };
  }
  return document;
}

/**
 * Refresh only while the page is visible.
 *
 * No timer runs while hidden. Becoming visible refreshes once at once (the data may be minutes
 * old) and then keeps the regular cadence; the timer is rebuilt so it cannot fire immediately
 * after a long hidden period.
 */
export function pollVisible(
  run: () => void,
  everyMs: number,
  host: VisibilityHost = browserHost(),
): () => void {
  let timer: ReturnType<typeof setInterval> | null = null;
  const stopTimer = () => {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
  };
  const onVisibility = () => {
    if (host.visibilityState === 'hidden') {
      stopTimer();
      return;
    }
    run();
    if (timer === null) timer = setInterval(run, everyMs);
  };

  host.addEventListener('visibilitychange', onVisibility);
  if (host.visibilityState !== 'hidden') {
    timer = setInterval(run, everyMs);
  }
  return () => {
    stopTimer();
    host.removeEventListener('visibilitychange', onVisibility);
  };
}
