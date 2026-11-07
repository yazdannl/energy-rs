/** A repeating refresh; returns its own teardown so a page can use it from `onMount`. */
export function poll(run: () => void, everyMs = 300_000): () => void {
  const timer = setInterval(run, everyMs);
  return () => clearInterval(timer);
}
