/**
 * Hash routing.
 *
 * The UI is served from one `index.html` under an ingress path it cannot know at build time, so
 * `#/plan` is the only route that keeps `base: './'` assets resolvable from every screen.
 */

export interface RouteDef {
  path: string;
  title: string;
  summary: string;
}

export const ROUTES: RouteDef[] = [
  { path: '/', title: 'Now', summary: 'Live flow, the current slot and today’s money' },
  { path: '/plan', title: 'Plan', summary: '30 h of quarter-hour decisions from the optimizer' },
  { path: '/prices', title: 'Prices', summary: '48 h of import and feed-in prices' },
  { path: '/batteries', title: 'Batteries', summary: 'Packs, learning, protection and overrides' },
  { path: '/savings', title: 'Savings', summary: 'Measured accounting, never planned values' },
  { path: '/settings', title: 'Settings', summary: 'Mode, contract, provider and limits' },
];

function parse(hash: string): string {
  const raw = hash.replace(/^#/, '');
  const path = raw.split('?')[0];
  const known = ROUTES.some((route) => route.path === path);
  return known ? path : '/';
}

function initial(): string {
  return typeof location === 'undefined' ? '/' : parse(location.hash);
}

export const router = $state<{ path: string }>({ path: initial() });

if (typeof window !== 'undefined') {
  window.addEventListener('hashchange', () => {
    router.path = parse(location.hash);
  });
}

export function navigate(path: string): void {
  if (typeof location !== 'undefined') {
    location.hash = `#${path}`;
  }
  router.path = path;
}

export function routeOf(path: string): RouteDef {
  return ROUTES.find((route) => route.path === path) ?? ROUTES[0];
}
