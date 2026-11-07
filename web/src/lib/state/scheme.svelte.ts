/**
 * The OS colour-scheme preference, as a rune.
 *
 * CSS handles the palette; charts draw on a canvas and cannot read CSS variables, so they need
 * this to re-create themselves with the right ink colour.
 */

export type Scheme = 'dark' | 'light';

const media =
  typeof window === 'undefined' ? null : window.matchMedia('(prefers-color-scheme: light)');

export const scheme = $state<{ current: Scheme }>({
  current: media?.matches === true ? 'light' : 'dark',
});

media?.addEventListener('change', (event) => {
  scheme.current = event.matches ? 'light' : 'dark';
});
