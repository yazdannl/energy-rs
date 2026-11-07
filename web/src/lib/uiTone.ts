/**
 * Tone maps.
 *
 * Tailwind scans source text, so these are complete class strings, never string concatenation:
 * an interpolated `bg-${tone}-500` would silently lose its CSS in the build.
 */

export type Tone = 'neutral' | 'muted' | 'good' | 'warn' | 'bad' | 'info' | 'pv' | 'batt' | 'grid';

export const BADGE: Record<Tone, string> = {
  neutral: 'border-line-strong/70 bg-panel-soft text-muted',
  muted: 'border-line bg-panel-soft text-faint',
  good: 'border-good/30 bg-good/10 text-good',
  warn: 'border-warn/30 bg-warn/10 text-warn',
  bad: 'border-bad/30 bg-bad/10 text-bad',
  info: 'border-batt/30 bg-batt/10 text-batt',
  pv: 'border-pv/30 bg-pv/10 text-pv',
  batt: 'border-batt/30 bg-batt/10 text-batt',
  grid: 'border-grid/30 bg-grid/10 text-grid',
};

export const TEXT: Record<Tone, string> = {
  neutral: 'text-ink',
  muted: 'text-faint',
  good: 'text-good',
  warn: 'text-warn',
  bad: 'text-bad',
  info: 'text-batt',
  pv: 'text-pv',
  batt: 'text-batt',
  grid: 'text-grid',
};

export const ICON_TILE: Record<Tone, string> = {
  neutral: 'bg-line/60 text-muted',
  muted: 'bg-line/40 text-faint',
  good: 'bg-good/10 text-good',
  warn: 'bg-warn/10 text-warn',
  bad: 'bg-bad/10 text-bad',
  info: 'bg-batt/10 text-batt',
  pv: 'bg-pv/10 text-pv',
  batt: 'bg-batt/10 text-batt',
  grid: 'bg-grid/10 text-grid',
};

export const FILL: Record<Tone, string> = {
  neutral: 'bg-muted',
  muted: 'bg-faint',
  good: 'bg-good',
  warn: 'bg-warn',
  bad: 'bg-bad',
  info: 'bg-batt',
  pv: 'bg-pv',
  batt: 'bg-batt',
  grid: 'bg-grid',
};

export const BUTTON: Record<'primary' | 'ghost' | 'danger', string> = {
  primary: 'bg-ink text-base hover:bg-ink/90 border border-transparent',
  ghost: 'border border-line-strong/70 bg-panel-soft text-ink hover:border-line-strong',
  danger: 'border border-bad/40 bg-bad/10 text-bad hover:bg-bad/15',
};
