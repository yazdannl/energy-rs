/**
 * The Batteries tab card must render both a real cell report and a pack with no packData.
 * Svelte's server renderer is enough here: the card is pure derived state, no browser APIs.
 */

import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';

import { makePacks } from '../../lib/api/mock/fixtures';

import PackCard from './PackCard.svelte';

const packs = makePacks(new Date('2026-06-01T12:00:00Z')).packs;

function card(index: number): string {
  const pack = packs[index];
  if (!pack) throw new Error('mock pack missing');
  return render(PackCard, { props: { pack, onchanged: () => {} } }).body;
}

describe('PackCard cell voltages', () => {
  it('shows the cells line and the guard badge for a pack with packData', () => {
    const html = card(0);
    expect(html).toContain('cells 3.35 / 3.32 V · Δ 30 mV · pack 49.7 V · 29.1 °C');
    expect(html).toContain('cells ok');
    expect(html).toContain('title="3.35 / 3.32 V');
  });

  it('shows an em dash and no badge when packData is empty', () => {
    const html = card(1);
    expect(html).toContain('cells —');
    expect(html).not.toContain('NaN');
    expect(html).not.toContain('null');
    expect(html).not.toContain('cells ok');
  });
});
