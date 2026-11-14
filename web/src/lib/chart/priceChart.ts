//! The buy and feed-in prices, in their own short strip under the power panel: same width, same
//! time axis and the same cursor as the panel above, on an axis wide enough for the euro labels.

import uPlot from 'uplot';

import type { Plan } from '../api/types';
import { epochSeconds } from '../format/time';
import { PANEL_LEFT, PRICE_SYNC_KEY, SOC_AXIS_SIZE, axisStyle, niceAxis } from './axis';
import type { PlanChartModel } from './planChart';
import type { ChartTheme } from './theme';

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
        size: SOC_AXIS_SIZE,
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
