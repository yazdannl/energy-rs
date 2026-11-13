/** Time formatting in the browser's local zone (the ledger stores UTC, days are Amsterdam). */

import { NO_VALUE, orNo } from './empty';

const time = new Intl.DateTimeFormat(undefined, {
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
});
const timeSeconds = new Intl.DateTimeFormat(undefined, {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
});
const weekday = new Intl.DateTimeFormat(undefined, { weekday: 'short' });
const dayMonth = new Intl.DateTimeFormat(undefined, { day: 'numeric', month: 'short' });
const dayMonthYear = new Intl.DateTimeFormat(undefined, {
  day: 'numeric',
  month: 'short',
  year: 'numeric',
});

export function parseInstant(instant: string): Date {
  return new Date(instant);
}

/** `14:15` for a slot or a timestamp. */
export function clock(instant: string | null | undefined): string {
  return orNo(instant, (value) => time.format(parseInstant(value)));
}

/** `14:15:30` for a tick or a sample. */
export function clockSeconds(instant: string | null | undefined): string {
  return orNo(instant, (value) => timeSeconds.format(parseInstant(value)));
}

/**
 * `18:45 → 00:00` for a same-day span, `18:45 → 00:00 (+1d)` when it crosses local midnight.
 *
 * A 30-hour horizon almost always crosses it, and the times alone made it read as a five-hour plan.
 */
export function span(start: string | null | undefined, end: string | null | undefined): string {
  if (start === null || start === undefined || end === null || end === undefined) {
    return `${clock(start)} → ${clock(end)}`;
  }
  const from = parseInstant(start);
  const to = parseInstant(end);
  const days = Math.round(
    (new Date(to.getFullYear(), to.getMonth(), to.getDate()).getTime() -
      new Date(from.getFullYear(), from.getMonth(), from.getDate()).getTime()) /
      86_400_000,
  );
  const suffix = days <= 0 ? '' : ` (+${days}d)`;
  return `${time.format(from)} → ${time.format(to)}${suffix}`;
}

/** `12 Jan` */
export function dayShort(value: string | null | undefined): string {
  return orNo(value, (iso) => dayMonth.format(new Date(`${iso}T12:00:00`)));
}

/** `12 Jan 2026` */
export function dayLong(value: string | null | undefined): string {
  return orNo(value, (iso) => dayMonthYear.format(new Date(`${iso}T12:00:00`)));
}

/** `Mon 12 Jan` for a slot start. */
export function slotDay(instant: string): string {
  const date = parseInstant(instant);
  return `${weekday.format(date)} ${dayMonth.format(date)}`;
}

/** A relative age, capped at days; used for stale markers and last-tick labels. */
export function ago(instant: string | null | undefined, now = Date.now()): string {
  if (instant === null || instant === undefined) return NO_VALUE;
  const deltaS = Math.max(0, Math.round((now - parseInstant(instant).getTime()) / 1000));
  if (deltaS < 5) return 'just now';
  if (deltaS < 60) return `${deltaS} s ago`;
  if (deltaS < 3600) return `${Math.floor(deltaS / 60)} min ago`;
  if (deltaS < 86_400) return `${Math.floor(deltaS / 3600)} h ago`;
  return `${Math.floor(deltaS / 86_400)} d ago`;
}

/** Seconds since epoch, the x axis uPlot wants. */
export function epochSeconds(instant: string): number {
  return Math.floor(parseInstant(instant).getTime() / 1000);
}
