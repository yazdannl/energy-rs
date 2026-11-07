/**
 * Canvas colours.
 *
 * A uPlot canvas cannot read CSS custom properties, so the two palettes are duplicated here with
 * the same intent as `app.css`: dark-first, light only when the OS asks for it. Charts are
 * re-created when the scheme flips (see `PlanChart.svelte`).
 */

import type { Scheme } from '../state/scheme.svelte';

export interface ChartTheme {
  ink: string;
  muted: string;
  grid: string;
  axis: string;
  buy: string;
  feedIn: string;
  pv: string;
  load: string;
  battery: string;
  soc: string;
  target: string;
  good: string;
  bad: string;
}

export const THEMES: Record<Scheme, ChartTheme> = {
  dark: {
    ink: '#e7eef9',
    muted: '#64748b',
    grid: 'rgba(148, 163, 184, 0.14)',
    axis: 'rgba(148, 163, 184, 0.55)',
    buy: '#dbe7f6',
    feedIn: '#2dd4bf',
    pv: '#f5c451',
    load: '#94a3b8',
    battery: '#38bdf8',
    soc: '#a78bfa',
    target: 'rgba(167, 139, 250, 0.45)',
    good: '#34d399',
    bad: '#fb7185',
  },
  light: {
    ink: '#0b1526',
    muted: '#7b879b',
    grid: 'rgba(100, 116, 139, 0.16)',
    axis: 'rgba(82, 96, 121, 0.65)',
    buy: '#1b2c44',
    feedIn: '#0f766e',
    pv: '#a16207',
    load: '#64748b',
    battery: '#0369a1',
    soc: '#6d28d9',
    target: 'rgba(109, 40, 217, 0.4)',
    good: '#047857',
    bad: '#be123c',
  },
};

export function themeFor(scheme: Scheme): ChartTheme {
  return THEMES[scheme];
}
