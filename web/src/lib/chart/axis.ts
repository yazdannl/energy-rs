/**
 * Shared axis helpers for the two plan charts: the tick styling, and bounds that a person can read.
 */

import type { ChartTheme } from './theme';

/** The tick, grid and font styling every plan axis shares. */
export function axisStyle(theme: ChartTheme, side: 0 | 1 | 2 | 3) {
  return {
    side,
    stroke: theme.muted,
    grid: { stroke: theme.grid, width: 1 },
    ticks: { stroke: theme.axis, width: 1, size: 4 },
    font: '11px ui-sans-serif, system-ui, sans-serif',
    labelFont: '11px ui-sans-serif, system-ui, sans-serif',
  };
}

/** Round bounds to a step a person can read, so the battery floor is a labelled gridline. */
export function niceAxis(min: number, max: number, target: number): [number, number] {
  const span = Math.max(max - min, 0.2);
  const raw = span / (target - 1);
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  // The closest 1-2-5 step at or just under the raw spacing: rounding *up* to the next step turns a
  // 0.33 € span into a 0.2 € gridline and drags the axis back to zero.
  const step =
    [0.5, 1, 2, 5, 10]
      .map((multiplier) => multiplier * magnitude)
      .find((value) => value >= raw * 0.8) ?? 10 * magnitude;
  return [Math.floor(min / step) * step, Math.ceil(max / step) * step];
}
