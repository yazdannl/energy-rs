/** The six screens, with the icon each one uses in the sidebar and the bottom bar. */

import BatteryCharging from '@lucide/svelte/icons/battery-charging';
import ChartLine from '@lucide/svelte/icons/chart-line';
import Euro from '@lucide/svelte/icons/euro';
import Gauge from '@lucide/svelte/icons/gauge';
import PiggyBank from '@lucide/svelte/icons/piggy-bank';
import Settings from '@lucide/svelte/icons/settings';
import type { Component } from 'svelte';

export interface NavItem {
  path: string;
  label: string;
  icon: Component<{ class?: string }>;
}

export const NAV_ITEMS: NavItem[] = [
  { path: '/', label: 'Now', icon: Gauge },
  { path: '/plan', label: 'Plan', icon: ChartLine },
  { path: '/prices', label: 'Prices', icon: Euro },
  { path: '/batteries', label: 'Batteries', icon: BatteryCharging },
  { path: '/savings', label: 'Savings', icon: PiggyBank },
  { path: '/settings', label: 'Settings', icon: Settings },
];
