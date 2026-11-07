# Savings accounting

`GET /api/v1/savings` answers one question with measurements only: **what did the battery actually
save, per day and over the lifetime?** Nothing here is derived from a plan, an intended setpoint or
a forecast. The method is the no-battery counterfactual, settled with the same prices and the same
saldering rules as the real meter.

Code: `crates/energy-core/src/savings` (method and rollup), `crates/energy-ledger/src/savings`
(storage), `crates/energy-server/src/api/savings.rs` (the DTO). PLAN.md §8 is the requirement.

## Per slot

The day is a grid of 15-minute slots from local midnight to the next local midnight (92 on the
spring-forward day, 100 on the autumn day). For every slot the rollup reads the energy dashboard's
`change` values — grid import, grid export, PV — and the battery charge/discharge counters. The
house load is the dashboard's own balance, never the sum of device meters:

```
load = import - export + pv + discharge - charge
```

A slot counts as measured only when import, export, PV and load are all present and the derived load
is finite and above −0.005 kWh (a few Wh of disagreement between independent accumulators is
rounding; more means a counter does not add up). A missing row is a gap, not a zero. A battery
counter is optional (a dashboard without a battery reports 0); a **configured** counter with a hole
makes the slot a gap.
The slot's all-in import price and market price come from the price store's published slot. The
measurement source is the energy dashboard's own statistics, not the ledger's 1-minute pack tier:
the dashboard is the authority for the grid and PV counters (and for the load balance), while the
ledger's samples are per-pack telemetry with 30-day retention.

## The two legs

Every measured slot is settled twice with `energy_price::Settlement`, in slot order:

* **actual** — the measured slot **net** `import - export` is settled: an importing slot pays
  `net × buy`, an exporting slot is settled with netting and the Zonnebonus included. Export is
  `Pv` for a Zonnebonus when the slot produced PV, `Battery` otherwise.
* **baseline** — the same PV, the same load, the same prices, **no battery**:
  `net_base = load - pv`, so `import_base = max(0, net_base)` and
  `export_base = max(0, -net_base)`, export settled as PV.

`actual_eur = import_cost - export_revenue`, `baseline_eur` likewise, and
`saving_eur = baseline_eur - actual_eur`.

Settlement is on the slot net, never on the two registers separately: a slot whose meter imported and
exported energy is one net flow, and pricing both would invent money (prices.md §4.3). The stored
`import_kwh`/`export_kwh` columns are still the measured register sums, so they can exceed the netted
energy on such a slot; the money is the netted truth. A negative import price pays you; a negative
export price is refused (clamped to zero) exactly as the optimizer's `Settlement` does.

## Saldering budget

The budget is the annual `import_ytd - netted_ytd`, and it changes during the day, so both legs
replay in slot order: each slot's import first raises the meter, then the slot's export consumes
whatever budget remains. `Settlement::settle` does the netting and the bonus counter.

The start-of-day counters are rebuilt from the energy dashboard's **hourly** import/export history
since local January 1, grouped by local day and accumulated in day order (a day's imports count
before its exports). The counter is extended one day at a time and cached in the process, so a
catch-up reads January once and a running add-on reads one day. The Zonnebonus counter starts at the
year's non-netted export total.

Two documented approximations follow from not having the utility meter:

1. **Day granularity across days.** The real meter nets per slot all year; here only the day being
   accounted is replayed slot by slot, and the year-to-date counters are advanced from daily totals.
   The budget can therefore be off by the netting of the slots inside other days.
2. **The counterfactual's year is not replayed.** The baseline starts each day from the *actual*
   world's counters and then advances its own import meter with its own (larger) imports and its own
   exports. That is self-consistent within the day; it deliberately does not invent a whole
   battery-less year.

A recorder gap in the annual history understates `import_ytd` (a smaller budget, never a phantom
export). A day with no annual history at all still nets against its own imports; starting the year
mid-install understates the budget, it never overstates a saving.

## The four-way split

The saving is split into four components that add up to it **exactly** (the storage rounds each to
1e-6 and lets the balance term carry the rounding):

| field | definition |
|---|---|
| `arbitrage_eur` | `Σ (discharge - charge) × (buy - reference)`: the battery's time shift against the day's flat reference price (the mean all-in price of the day's published slots). |
| `self_consumption_eur` | `Σ (export_base - export_actual) × buy`: PV kept on site instead of exported, valued at the import price of the slot whose net export it replaced. |
| `feed_in_eur` | `Σ (export_revenue - export_revenue_base)`: what exported kWh actually earned, actual versus baseline. This is where saldering, the bonus and negative prices land. |
| `battery_balance_eur` | `Σ (discharge - charge) × reference`: the battery's own energy balance at the reference price — round-trip loss (negative while cycling) and a pack that ends the day fuller or emptier than it started. |

Why these four are exact: the measured balance gives
`import_base - import_actual = (export_base - export_actual) + (discharge - charge)` per slot, so paying
the import difference at `buy` and adding the export revenue difference already equals the saving.
Splitting the battery term at the reference price is pure algebra.
`arbitrage + self_consumption + feed_in + battery_balance == saving` is enforced by a test on every
scenario and on the stored row.

Read the terms together, not in isolation: under full netting, keeping a kWh of PV on site appears as
`+buy` in self-consumption and `-buy` in the feed-in effect, which is the honest answer (with 1:1
netting, self-consumption saves nothing). On a PV-surplus day the self-consumption term values the
kept kWh at the import price while the feed-in term removes the export revenue it displaced, so the
pair nets down to the avoided import minus the lost feed-in.

## Completeness and `NULL`

A day is **complete** only when every slot of the local day has a price and an import, export, PV and
load measurement. Then:

* a closed complete day is stored with its money and `complete = 1`;
* a closed day with any gap stores `NULL` money, its measured energy sums and its `coverage_hours`
  (measured slots × 0.25 h) and `complete = 0` — never a number that looks like a saving;
* the **current day** is provisional: it keeps its measured-so-far money, `complete = 0`, and is
  recomputed every quarter-hour. The API's `period=day` is that provisional value.

`lifetime` totals sum complete days only; `month`/`year` also include the provisional current day;
a `NULL` day contributes to no total. Energy sums over a `NULL` day are the measured part and are not
in any total either.

## Rollups

`SavingsRollup::refresh(now)` recomputes, in day order:

1. the day after the newest stored closed day;
2. that newest day again when it was stored incomplete (late statistics can still fill it);
3. the current day.

With no rows at all it looks back at most 31 days (`CATCH_UP_DAYS`), so a stopped add-on catches up
when it boots. Writes are upserts: the same measurements produce byte-identical rows, and a day that
lost coverage has its numbers replaced by `NULL`. A day that cannot be read (Home Assistant down, a
missing statistic) is logged and skipped; the rest of the pass still runs and the next quarter
retries it. The loop runs in the server's bootstrap, never inside the control tick.

## Deliberately not counted

* **Fixed charges** — standing charge, capacity tariff, meter rent. They do not change with the
  battery, so counting them would inflate or deflate the saving.
* **Tax reductions** — the energy-tax rebate (`heffingskorting`/`vermindering`) and any VAT refund
  scheme. The settlement prices are the published all-in prices; no yearly rebate is spread over
  kWh.
* **Battery degradation** — cycle wear, calendar ageing, capacity fade. The pack is a cost of
  ownership, not an energy flow; discounting it would require a wear model this system does not have.
* **Investment and payback** — purchase, installation, financing, subsidies.
* **Self-discharge and standby consumption** inside the battery when no counter measures it.
* **Curtailment or refused export** — energy the inverter never produced or the meter never saw.
* **Value of avoided CO₂, comfort or resilience.**
* **Planned or intended flows** — if the add-on was in `advisory`/`dry_run` and did not write, the
  measurement shows no battery activity and the saving is honestly zero.
