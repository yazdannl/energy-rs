/**
 * The 30 h plan timeline: forecast areas, battery power bars and the SoC trajectory.
 *
 * Series order is fixed and every layer is always present; the toggles hide a series with
 * `u.setSeries(i, {show})` instead of rebuilding the chart, so data and cursor stay stable.
 *
 * Two labelled scales in the main panel: kilowatts on the left for PV, load and the battery, and
 * kilowatt hours on the right for the state of charge. A second kilowatt axis would be a second
 * column of the same unit, which is unreadable on a 400 px phone; a third one is worse. Prices get
 * their own strip underneath (see [`planPriceChart`]) sharing the time axis, so the euro scale is
 * legible without competing with the kilowatts.
 *
 * SoC is a third quantity — an amount, not a rate — so it gets a third labelled axis rather than a
 * hidden scale that would make 3.5 kWh look like 1.6 kW. The axis reads in percent of the fleet's
 * capacity (the plan carries each slot's percentage), because that is the number an operator checks;
 * the exact kWh stay in the readout.
 */

import uPlot from 'uplot';

import type { Plan } from '../api/types';
import { clock, epochSeconds } from '../format/time';
import { axisStyle, niceAxis } from './axis';
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

/** The cursor sync key shared by the power panel and the price strip. */
export const PRICE_SYNC_KEY = 'plan-timeline';

/**
 * The power panel's plot starts 56 px in (4 px padding plus its 52 px axis) and ends 58 px before
 * the right edge (4 px padding plus its 54 px axis). The price strip has no left axis, so it buys
 * the same 56 px with padding; its own 54 px axis and 4 px padding finish the right side.
 */
const PANEL_LEFT = 56;

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
  // State of charge reads as a percentage of the fleet: the plan carries it per slot against the
  // capacity the solver used, and any slot with both numbers recovers that capacity exactly.
  const socCeilingKwh = slots.reduce<number | null>((ceiling, slot) => {
    if (ceiling !== null || slot.socPct === null || slot.socPct <= 0 || slot.socWh <= 0) {
      return ceiling;
    }
    return slot.socWh / (slot.socPct / 100) / 1000;
  }, null);
  const socValue = (wh: number): number =>
    socCeilingKwh === null ? wh / 1000 : (wh / 1000 / socCeilingKwh) * 100;
  const soc = slots.map((slot) => socValue(slot.socWh));
  const target = slots.map((slot) => socValue(slot.targetSocWh));

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
      value: (_, value) =>
        socCeilingKwh === null ? `${value.toFixed(2)} kWh` : `${value.toFixed(0)} %`,
    },
    {
      label: 'Target SoC',
      scale: 'soc',
      stroke: theme.target,
      width: 1,
      dash: [3, 4],
      points: { show: false },
      value: (_, value) =>
        socCeilingKwh === null ? `${value.toFixed(2)} kWh` : `${value.toFixed(0)} %`,
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
      // The price strip below subscribes to this cursor, so its crosshair sits on the same
      // quarter-hour without it being interactive itself.
      sync: { key: PRICE_SYNC_KEY },
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
      soc: {
        range: (_u, _min, max) =>
          socCeilingKwh === null ? niceAxis(0, Math.max(max, 0.2), 5) : [0, 100],
      },
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
        values: (_, splits) =>
          splits.map((value) =>
            socCeilingKwh === null ? `${value.toFixed(1)} kWh` : `${Math.round(value)} %`,
          ),
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

/**
 * The buy and feed-in prices, in their own short strip under the power panel.
 *
 * Same width, same time axis and the same cursor as the panel above; the padding reproduces the
 * main chart's axis indents so a quarter-hour sits at the same x in both.
 */
export function planPriceChart(plan: Plan, theme: ChartTheme): PlanChartModel {
  const xs = plan.slots.map((slot) => epochSeconds(slot.startAt));
  const buy = plan.slots.map((slot) => slot.buyEurPerKwh);
  const feedIn = plan.slots.map((slot) => slot.feedInEurPerKwh);
  const data: uPlot.AlignedData = [xs, buy, feedIn] as uPlot.AlignedData;

  const options: uPlot.Options = {
    width: 640,
    height: 76,
    padding: [8, 4, 2, PANEL_LEFT],
    legend: { show: false },
    cursor: {
      points: { size: 4 },
      y: false,
      drag: { x: false, y: false, setScale: false },
      sync: { key: PRICE_SYNC_KEY },
    },
    tzDate: (ts) => new Date(ts * 1000),
    scales: {
      x: { time: true },
      price: {
        // Prices never start at zero in a day-ahead tariff; padding the real range keeps the shape
        // readable instead of pinning it to a baseline that no slot uses.
        range: (_u, min, max) => {
          const pad = (max - min) * 0.05;
          return niceAxis(min - pad, max + pad, 4);
        },
      },
    },
    axes: [
      { ...axisStyle(theme, 2), show: false },
      {
        ...axisStyle(theme, 1),
        scale: 'price',
        size: 54,
        space: 24,
        values: (_, splits) => splits.map((value) => `€${value.toFixed(2)}`),
      },
    ],
    series: [
      {
        label: 'Time',
        value: (_, value) => new Date(value * 1000).toLocaleTimeString(),
      },
      {
        label: 'Buy',
        scale: 'price',
        stroke: theme.buy,
        width: 1.4,
        points: { show: false },
        value: (_, value) => `€${value.toFixed(3)}`,
      },
      {
        label: 'Feed-in',
        scale: 'price',
        stroke: theme.feedIn,
        width: 1.2,
        dash: [4, 3],
        points: { show: false },
        value: (_, value) => `€${value.toFixed(3)}`,
      },
    ],
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
