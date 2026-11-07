/**
 * `prefers-reduced-motion`, as a rune.
 *
 * CSS already disables animations and transitions; this exists for the places where a duration is
 * a JavaScript number (Svelte transitions), where "0 ms" is the honest answer.
 */

const media =
  typeof window === 'undefined' ? null : window.matchMedia('(prefers-reduced-motion: reduce)');

export const motion = $state<{ reduced: boolean }>({ reduced: media?.matches === true });

media?.addEventListener('change', (event) => {
  motion.reduced = event.matches;
});

/** A transition duration that is zero when the OS asks for no motion. */
export function duration(ms = 180): number {
  return motion.reduced ? 0 : ms;
}
