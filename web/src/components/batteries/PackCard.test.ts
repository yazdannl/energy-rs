/**
 * The Batteries tab card must render both a real cell report and a pack with no packData.
 * Svelte's server renderer is enough here: the card is pure derived state, no browser APIs.
 */

import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';

import { makePacks } from '../../lib/api/mock/fixtures';

import PackCard from './PackCard.svelte';

const packs = makePacks(new Date('2026-06-01T12:00:00Z')).packs;
const now = Date.parse('2026-06-01T12:00:00Z');

function card(index: number, at = now): string {
  const pack = packs[index];
  if (!pack) throw new Error('mock pack missing');
  return render(PackCard, { props: { pack, onchanged: () => {}, now: at } }).body;
}

describe('PackCard cell voltages', () => {
  it('shows both wrapped rows and a fresh age for a pack with packData', () => {
    const html = card(0);
    expect(html).toContain('cells 3.35 / 3.32 V · Δ 30 mV');
    expect(html).toContain('4 s ago');
    expect(html).toContain('pack 49.7 V · 29.1 °C');
    expect(html).toContain('title="3.35 / 3.32 V · Δ 30 mV · pack 49.7 V · 29.1 °C · 4 s ago"');
    expect(html).toContain('num min-w-0 break-words');
    expect(html).toContain('text-faint');
    expect(html).not.toContain('below cutoff');
    expect(html).not.toContain('cells ok');
  });

  it('shows em dashes and an amber age when the reading is older than three ticks', () => {
    const stale = packs[1];
    if (!stale?.state?.sampledAt) throw new Error('stale mock pack missing');
    const html = card(1, Date.parse(stale.state.sampledAt) + 22_000);
    expect(html).toContain('cells —');
    expect(html).toContain('pack — V · — °C');
    expect(html).toContain('22 s ago');
    expect(html).toContain('text-warn');
    expect(html).toContain('title="— · 22 s ago"');
    expect(html).not.toContain('NaN');
    expect(html).not.toContain('null');
  });

  it('renders a changed cell value in the same rows', () => {
    const pack = packs[0];
    if (!pack?.state?.cells) throw new Error('fresh mock pack missing');
    const changed = {
      ...pack,
      state: {
        ...pack.state,
        cells: { ...pack.state.cells, maxCellV: 3.41, spreadMv: 90 },
      },
    };
    const html = render(PackCard, { props: { pack: changed, onchanged: () => {}, now } }).body;

    expect(html).toContain('cells 3.41 / 3.32 V · Δ 90 mV');
    expect(html).not.toContain('3.35 / 3.32 V');
    expect(html).toContain('title="3.41 / 3.32 V · Δ 90 mV · pack 49.7 V · 29.1 °C · 4 s ago"');
  });
});
