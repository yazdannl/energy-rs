/**
 * The 30 h plan timeline: price lines, forecast areas, battery power bars and the SoC trajectory.
 *
 * Series order is fixed and every layer is always present; the toggles hide a series with
 * `u.setSeries(i, {show})` instead of rebuilding the chart, so data and cursor stay stable.
 *
 * SoC is plotted in **kWh** on its own hidden scale. A percentage would need the pack capacity,
 * and the plan DTO does not carry it — a fabricated axis maximum is exactly how a chart starts
 * lying.
 */

import uPlot from 'uplot';

import type { Plan } from '../api/types';
import { clock, epochSeconds } from '../format/time';
import type { ChartTheme } from './theme';

export interface PlanLayers {
  prices: boolean;
  battery: boolean;
  soc: boolean;
  forecast: boolean;
}

export const DEFAULT_LAYERS: PlanLayers = {
  prices: true,
  battery: true,
  soc: true,
  forecast: true,
};

/** Series indices in `data`, in the order `planChart` builds them. */
export const SERIES_INDEX = {
  buy: 1,
  feedIn: 2,
  pv: 3,
  load: 4,
  battery: 5,
  soc: 6,
  target: 7,
} as const;

export interface PlanChartModel {
  data: uPlot.AlignedData;
  options: uPlot.Options;
}

function axisStyle(theme: ChartTheme, side: 0 | 1 | 2 | 3) {
  return {
    side,
    stroke: theme.muted,
    grid: { stroke: theme.grid, width: 1 },
    ticks: { stroke: theme.axis, width: 1, size: 4 },
    font: '11px ui-sans-serif, system-ui, sans-serif',
    labelFont: '11px ui-sans-serif, system-ui, sans-serif',
  };
}

export function planChart(
  plan: Plan,
  theme: ChartTheme,
  onCursor: (index: number | null) => void,
): PlanChartModel {
  const slots = plan.slots;
  const xs = slots.map((slot) => epochSeconds(slot.startAt));
  const buy = slots.map((slot) => slot.buyEurPerKwh);
  const feedIn = slots.map((slot) => slot.feedInEurPerKwh);
  const pv = slots.map((slot) => (slot.pvWh * 4) / 1000);
  const load = slots.map((slot) => (slot.loadWh * 4) / 1000);
  const battery = slots.map((slot) => slot.batteryPowerW / 1000);
  const soc = slots.map((slot) => slot.socWh / 1000);
  const target = slots.map((slot) => slot.targetSocWh / 1000);

  const data: uPlot.AlignedData = [
    xs,
    buy,
    feedIn,
    pv,
    load,
    battery,
    soc,
    target,
  ] as uPlot.AlignedData;
  const socMax = Math.max(0.1, ...soc, ...target) * 1.12;
  const barPaths = uPlot.paths.bars?.({ size: [0.66, 14], align: 0, radius: 0.25 });

  const series: uPlot.Series[] = [
    {
      label: 'Time',
      value: (_, value) => new Date(value * 1000).toLocaleTimeString(),
    },
    {
      label: 'Buy',
      scale: 'price',
      stroke: theme.buy,
      width: 1.6,
      points: { show: false },
      value: (_, value) => `€${value.toFixed(3)}`,
    },
    {
      label: 'Feed-in',
      scale: 'price',
      stroke: theme.feedIn,
      width: 1.2,
      dash: [5, 4],
      points: { show: false },
      value: (_, value) => `€${value.toFixed(3)}`,
    },
    {
      label: 'PV forecast',
      scale: 'power',
      stroke: theme.pv,
      width: 1,
      fill: 'rgba(245, 196, 81, 0.16)',
      points: { show: false },
      value: (_, value) => `${value.toFixed(2)} kW`,
    },
    {
      label: 'Load forecast',
      scale: 'power',
      stroke: theme.load,
      width: 1,
      dash: [2, 3],
      points: { show: false },
      value: (_, value) => `${value.toFixed(2)} kW`,
    },
    {
      label: 'Battery power',
      scale: 'power',
      stroke: theme.battery,
      fill: 'rgba(56, 189, 248, 0.55)',
      width: 1,
      points: { show: false },
      ...(barPaths === undefined ? {} : { paths: barPaths }),
      fillTo: () => 0,
      value: (_, value) => `${value >= 0 ? '+' : ''}${value.toFixed(2)} kW`,
    },
    {
      label: 'SoC',
      scale: 'soc',
      stroke: theme.soc,
      width: 2,
      fill: 'rgba(167, 139, 250, 0.1)',
      points: { show: false },
      value: (_, value) => `${value.toFixed(2)} kWh`,
    },
    {
      label: 'Target SoC',
      scale: 'soc',
      stroke: theme.target,
      width: 1,
      dash: [3, 4],
      points: { show: false },
      value: (_, value) => `${value.toFixed(2)} kWh`,
    },
  ];

  const options: uPlot.Options = {
    width: 640,
    height: 280,
    padding: [12, 4, 2, 4],
    legend: { show: false },
    cursor: {
      points: { size: 5 },
      y: false,
      // The chart is panned and zoomed by the component (wheel to zoom, drag to pan); a drag box
      // would fight the pan.
      drag: { x: false, y: false, setScale: false },
    },
    tzDate: (ts) => new Date(ts * 1000),
    scales: {
      x: { time: true },
      price: { auto: true },
      power: { auto: true },
      soc: { range: [0, socMax] },
    },
    axes: [
      {
        ...axisStyle(theme, 2),
        size: 24,
        space: 70,
        incrs: [900, 1800, 3600, 7200, 10800, 21600],
        // Hour:minute labels, so the chart says *when* a slot is, not just where it sits.
        values: (_, splits) => splits.map((value) => clock(new Date(value * 1000).toISOString())),
      },
      {
        ...axisStyle(theme, 3),
        scale: 'price',
        size: 54,
        space: 44,
        values: (_, splits) => splits.map((value) => `€${value.toFixed(2)}`),
      },
      {
        ...axisStyle(theme, 1),
        scale: 'power',
        size: 46,
        space: 40,
        values: (_, splits) => splits.map((value) => `${value.toFixed(0)} kW`),
      },
      { ...axisStyle(theme, 1), scale: 'soc', show: false, size: 0 },
    ],
    series,
    hooks: {
      setCursor: [
        (self) => {
          const index = self.cursor.idx;
          onCursor(index === undefined ? null : index);
        },
      ],
    },
  };

  return { data, options };
}

/** Visibility for the layer toggles, as `[seriesIndex, visible]` pairs. */
export function layerVisibility(layers: PlanLayers): [number, boolean][] {
  return [
    [SERIES_INDEX.buy, layers.prices],
    [SERIES_INDEX.feedIn, layers.prices],
    [SERIES_INDEX.pv, layers.forecast],
    [SERIES_INDEX.load, layers.forecast],
    [SERIES_INDEX.battery, layers.battery],
    [SERIES_INDEX.soc, layers.soc],
    [SERIES_INDEX.target, layers.soc],
  ];
}
