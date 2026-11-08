/**
 * The API root has to survive the ingress path Home Assistant hands the panel.
 *
 * The Supervisor reports `ingress_url` as `/api/hassio_ingress/<token>//`, and resolving `api/v1/`
 * against that base naively produces `//api/v1/...`: the SPA shell still loads (so the page looks
 * alive) while every request 404s. These cases pin the normalisation that prevents it.
 */

import { describe, expect, it } from 'vitest';

import { apiRoot, collapseSlashes } from './client';

describe('apiRoot', () => {
  it('keeps a single-slash ingress prefix', () => {
    expect(apiRoot('https://ha.local/api/hassio_ingress/abc/').toString()).toBe(
      'https://ha.local/api/hassio_ingress/abc/api/v1/',
    );
  });

  it('collapses the doubled slash Home Assistant reports', () => {
    expect(apiRoot('https://ha.local/api/hassio_ingress/abc//').toString()).toBe(
      'https://ha.local/api/hassio_ingress/abc/api/v1/',
    );
  });

  it('serves the add-on root directly', () => {
    expect(apiRoot('http://addon:8099/').toString()).toBe('http://addon:8099/api/v1/');
  });

  it('builds the stream URL from the same root', () => {
    const root = apiRoot('https://ha.local/api/hassio_ingress/abc//');
    expect(new URL('stream', root).toString()).toBe(
      'https://ha.local/api/hassio_ingress/abc/api/v1/stream',
    );
  });
});

describe('collapseSlashes', () => {
  it('keeps the scheme separator', () => {
    expect(collapseSlashes('https://ha.local//x//y')).toBe('https://ha.local/x/y');
  });

  it('leaves an already clean URL alone', () => {
    expect(collapseSlashes('https://ha.local/a/b/')).toBe('https://ha.local/a/b/');
  });
});
