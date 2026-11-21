/** A refresh must update numbers in place: data stays visible while the next load is in flight. */

import { describe, expect, it } from 'vitest';

import { resource } from './resource.svelte';

describe('Resource refresh', () => {
  it('keeps the previous data during a refresh, so pages never flash their first-load skeleton', async () => {
    let value = 1;
    const refresh: { release?: (next: { value: number }) => void } = {};
    const packs = resource(
      () =>
        new Promise<{ value: number }>((resolve) => {
          if (value === 1) resolve({ value: 1 });
          else refresh.release = resolve;
        }),
    );

    await packs.load();
    expect(packs.data).toEqual({ value: 1 });

    value = 2;
    const refreshing = packs.load();
    expect(packs.loading).toBe(true);
    expect(packs.data).toEqual({ value: 1 });
    // This is the exact condition the page uses to choose Skeleton versus the pack grid.
    expect(packs.loading && packs.data === null).toBe(false);

    refresh.release?.({ value: 2 });
    await refreshing;
    expect(packs.data).toEqual({ value: 2 });
    expect(packs.loading).toBe(false);
    packs.dispose();
  });
});
