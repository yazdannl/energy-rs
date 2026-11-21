/** The shared visible-only poller: cadence, hidden pause/resume and clean teardown. */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { pollVisible, type VisibilityHost } from './poller';

class FakeHost implements VisibilityHost {
  visibilityState = 'visible';
  private readonly listeners = new Set<() => void>();

  addEventListener(type: string, listener: () => void): void {
    if (type !== 'visibilitychange') throw new Error(`unexpected event: ${type}`);
    this.listeners.add(listener);
  }

  removeEventListener(type: string, listener: () => void): void {
    if (type !== 'visibilitychange') throw new Error(`unexpected event: ${type}`);
    this.listeners.delete(listener);
  }

  emit(): void {
    for (const listener of [...this.listeners]) listener();
  }
}

describe('pollVisible', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('registers one 10 s interval and calls the loader on every tick', () => {
    const host = new FakeHost();
    const run = vi.fn();
    const stop = pollVisible(run, 10_000, host);

    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(20_000);
    expect(run).toHaveBeenCalledTimes(2);
    stop();
  });

  it('stops while hidden and refreshes once at once when visible again', () => {
    const host = new FakeHost();
    const run = vi.fn();
    const stop = pollVisible(run, 10_000, host);

    vi.advanceTimersByTime(10_000);
    expect(run).toHaveBeenCalledTimes(1);

    host.visibilityState = 'hidden';
    host.emit();
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(60_000);
    expect(run, 'no polling in a hidden tab').toHaveBeenCalledTimes(1);

    host.visibilityState = 'visible';
    host.emit();
    expect(run, 'one immediate refresh on return').toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(10_000);
    expect(run).toHaveBeenCalledTimes(3);
    stop();
  });

  it('starts with no timer when mounted into a hidden tab', () => {
    const host = new FakeHost();
    host.visibilityState = 'hidden';
    const run = vi.fn();
    const stop = pollVisible(run, 10_000, host);

    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(30_000);
    expect(run).not.toHaveBeenCalled();
    stop();
  });

  it('removes its listener and timer on teardown', () => {
    const host = new FakeHost();
    const run = vi.fn();
    const stop = pollVisible(run, 10_000, host);

    stop();
    expect(vi.getTimerCount()).toBe(0);
    host.visibilityState = 'hidden';
    host.emit();
    host.visibilityState = 'visible';
    host.emit();
    vi.advanceTimersByTime(30_000);
    expect(run).not.toHaveBeenCalled();
  });
});
