/**
 * The 30 h plan timeline: forecast areas, battery power bars and the SoC trajectory.
 *
 * Series order is fixed and every layer is always present; the toggles hide a series with
 * `u.setSeries(i, {show})` instead of rebuilding the chart, so data and cursor stay stable.
 *
 * Two labelled scales and no more: watts on the left for PV, load and the battery, and kilowatt
 * hours on the right for the state of charge. A second kilowatt axis would be a second column of
 * the same unit, which is unreadable on a 400 px phone; a third one is worse. Prices are not a
 * scale here either — they live in the readout under the chart and in the Prices tab.
 *
 * SoC is a third quantity — an amount, not a rate — so it gets a third labelled axis in kWh rather
 * than a hidden scale that would make 3.5 kWh look like 1.6 kW. A percentage would need the pack
 * capacity and the plan DTO does not carry it.
 */

import uPlot from 'uplot';

import type { Plan } from '../api/types';
import { clock, epochSeconds } from '../format/time';
import type { ChartTheme } from './theme';

export interface PlanLayers {
  battery: boolean;
  soc: boolean;
  forecast: boolean;
}

export const DEFAULT_LAYERS: PlanLayers = {
  battery: true,
  soc: true,
  forecast: true,
};

/** Series indices in `data`, in the order `planChart` builds them. */
export const SERIES_INDEX = {
  pv: 1,
  load: 2,
  battery: 3,
  soc: 4,
  target: 5,
} as const;

export interface PlanChartModel {
  data: uPlot.AlignedData;
  options: uPlot.Options;
}

/** Round bounds to a step a person can read, so the battery floor is a labelled gridline. */
function niceAxis(min: number, max: number, target: number): [number, number] {
  const span = Math.max(max - min, 0.2);
  const raw = span / (target - 1);
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const step =
    [0.5, 1, 2, 5, 10].map((multiplier) => multiplier * magnitude).find((value) => value >= raw) ??
    10 * magnitude;
  return [Math.floor(min / step) * step, Math.ceil(max / step) * step];
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
  const pv = slots.map((slot) => (slot.pvWh * 4) / 1000);
  const load = slots.map((slot) => (slot.loadWh * 4) / 1000);
  const battery = slots.map((slot) => slot.batteryPowerW / 1000);
  const soc = slots.map((slot) => slot.socWh / 1000);
  const target = slots.map((slot) => slot.targetSocWh / 1000);

  const data: uPlot.AlignedData = [xs, pv, load, battery, soc, target] as uPlot.AlignedData;
  const barPaths = uPlot.paths.bars?.({ size: [0.66, 14], align: 0, radius: 0.25 });

  const series: uPlot.Series[] = [
    {
      label: 'Time',
      value: (_, value) => new Date(value * 1000).toLocaleTimeString(),
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
      width: 1.4,
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
      power: {
        // Nice bounds, so the battery's discharge floor lands on a labelled gridline instead of
        // half a kilowatt past the last one. The battery discharges below zero, so the axis has to
        // reach under it as well.
        range: (_u, min, max) => niceAxis(Math.min(0, min), Math.max(max, 0.2), 6),
      },
      soc: { range: (_u, _min, max) => niceAxis(0, Math.max(max, 0.2), 5) },
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
        scale: 'power',
        size: 52,
        space: 34,
        values: (_, splits) => splits.map((value) => `${value.toFixed(1)} kW`),
      },
      {
        ...axisStyle(theme, 1),
        scale: 'soc',
        side: 1,
        size: 54,
        space: 48,
        grid: { show: false },
        values: (_, splits) => splits.map((value) => `${value.toFixed(1)} kWh`),
      },
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
    [SERIES_INDEX.pv, layers.forecast],
    [SERIES_INDEX.load, layers.forecast],
    [SERIES_INDEX.battery, layers.battery],
    [SERIES_INDEX.soc, layers.soc],
    [SERIES_INDEX.target, layers.soc],
  ];
}
