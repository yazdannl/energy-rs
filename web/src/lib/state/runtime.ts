/**
 * Which backend this page talks to.
 *
 * `?mock=1` (before or after the hash) selects fixtures so the UI can be developed and reviewed
 * without the add-on. Nothing else switches automatically: a failed fetch is shown as a failure.
 */

import { HttpApi, type ApiPort } from '../api/client';
import { MockApi, MockStream } from '../api/mock';
import { HttpStream, type StreamSource } from '../api/stream';

function flag(): boolean {
  if (typeof location === 'undefined') return false;
  const search = new URLSearchParams(location.search);
  if (search.get('mock') === '1') return true;
  const hash = location.hash.replace(/^#/, '');
  const query = hash.indexOf('?');
  return query >= 0 && new URLSearchParams(hash.slice(query + 1)).get('mock') === '1';
}

export const mockMode: boolean = flag();

export const api: ApiPort = mockMode ? new MockApi() : new HttpApi();
export const stream: StreamSource = mockMode ? new MockStream() : new HttpStream();
