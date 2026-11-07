/** What a missing number looks like: an em dash, never a zero that pretends to be a measurement. */

export const NO_VALUE = '—';

export function orNo<T>(value: T | null | undefined, render: (present: T) => string): string {
  return value === null || value === undefined ? NO_VALUE : render(value);
}
