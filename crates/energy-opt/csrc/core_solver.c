/* GENERATED FILE - DO NOT EDIT.
 *
 * The numeric core of the old add-on's CPython extension, extracted verbatim from
 *   ha-energy-manager/energy_manager/app/optimize/_core.c
 * (sha256 019c1d04194f45e7c527db86532a1f17bfaec7c35226e2eb1586e70e67125894), lines 43-50, 52-98, 100-535, 660-708.
 *
 * Regenerate with `python3 scripts/extract_core.py`; `--check` proves this file is current.
 * Everything below the include is byte-identical to the upstream ranges -- including the
 * `PyErr_SetString`/`PyErr_Format` call sites, which are mapped onto a local shim in
 * core_solver.h so the extracted text needs no editing.  The CPython entry point, the dict and
 * sequence marshalling and the config parser are deliberately NOT vendored:
 * csrc/energy_opt_ffi.c implements the C ABI.
 *
 * See csrc/UPSTREAM.md for what is extracted, what the shim covers and why.
 */
#include "core_solver.h"

/* Source identity for the loader (docs/OPTIMIZER_C_CORE.md): the build compiles in the SHA-256
 * prefix of the _core.c it was built from, so an artifact that was copied, restored or merely
 * touched cannot pass as a build of the current source (the digest covers the source, not the
 * compiler flags).  A manual gcc line may omit the define, and the loader then falls
 * back to comparing mtimes. */
#ifndef CORE_BUILD_SHA
#define CORE_BUILD_SHA ""
#endif
/* Mirror the dp.py constants; MAX_STATES / MAX_ITERATIONS arrive in the config so the Python
 * module stays the single source of truth for the iteration budget. */
#define EPS_KW 1e-6
#define EPS_KWH 1e-9
#define PV_AUTO 0
#define PV_PREFER 1
#define PV_AVOID 2

/*: Caller-supplied dimensions are capped so every size below fits (n+1)*m, 4*n and an int; a
 * larger value is rejected with ValueError before anything is narrowed.  The shipped solver uses
 * at most 400 states and a few hundred slots, so the caps are generous. */
#define CORE_MAX_DIMENSION 1000000L
#define CORE_MIN_STATES 8L

typedef struct {
    double e_min_kwh;
    double e_max_kwh;
    double p_charge_kw;
    double p_discharge_kw;
    double eta_charge;
    double eta_discharge;
    double start_kwh;
    int has_target;
    double e_target;
    double slot_minutes;
    double soc_step_kwh;
    long power_steps;
    int allow_grid_charge;
    int allow_export;
    /* Optimizer v2 stage 1b (docs/OPTIMIZER_V2.md section 2.3): the feed-in contract refused the
     * battery-sourced export leg.  Policy only; the Python reference's grid_window_kw is the
     * authority and _fast._validated rebuilds every bound from it (D1). */
    int refuse_battery_export;
    double max_import_kw;
    double max_export_kw;
    double min_spread_eur_per_kwh;
    int has_sell_floor;
    double sell_floor_eur_per_kwh;
    int pv_preference;
    double wear_eur_per_kwh;
    /*: ``wear_eur_per_kwh * eta_discharge``, precomputed because the value is identical. */
    double wear_rate;
    double salvage;
    double target_penalty;
    long max_states;
    long max_iterations;
    /*: Plan the two grid-neutral powers as actions as well: charge exactly the PV surplus and
     * serve exactly the house.  Off keeps the extracted core's action set (parity rows). */
    int align_actions;
} Config;
/* Per-slot constants, hoisted out of the state loop. */
typedef struct {
    double dt;
    double buy;
    double sell;
    double base;    /* load - pv */
    double surplus; /* max(0, pv - load) */
    double pv_price;
    double grid_lo;
    double grid_hi;
} SlotCtx;

static double clampd(double value, double low, double high) {
    return value < low ? low : (value > high ? high : value);
}

/* Checked size arithmetic: returns -1 instead of wrapping. */
static int checked_mul(size_t a, size_t b, size_t *out) {
    if (a != 0 && b > (size_t)-1 / a) return -1;
    *out = a * b;
    return 0;
}

static void insertion_sort(double *values, int n) {
    for (int i = 1; i < n; i++) {
        double key = values[i];
        int j = i - 1;
        while (j >= 0 && values[j] > key) {
            values[j + 1] = values[j];
            j--;
        }
        values[j + 1] = key;
    }
}

/* _build_grid: e_min -> e_max by step_kwh plus the anchors, coarsened to the state budget.
 * Every write is bounded by ``capacity`` and every cast sees only finite values. */
static int build_grid(double e_min, double e_max, double step_kwh, long max_states,
                      const double *anchors, int n_anchors, double *out, int capacity,
                      int *out_n, double *out_step) {
    if (capacity < 4) {
        PyErr_SetString(PyExc_ValueError, "state grid capacity is too small");
        return -1;
    }
    double span = e_max - e_min;
    if (span < 0.0) span = 0.0;
    if (!isfinite(span)) {
        PyErr_SetString(PyExc_ValueError, "soc bounds do not produce a finite span");
        return -1;
    }
    double step = step_kwh > 1e-4 ? step_kwh : 1e-4;
    double budget = (double)(max_states - 3);
    if (budget < 1.0) budget = 1.0;
    double ratio = span / step;
    if (!(ratio <= budget - 1.0)) step = span / (budget - 1.0);
    if (!(step > 0.0) || !isfinite(step)) {
        PyErr_SetString(PyExc_ValueError, "soc step is not usable for these bounds");
        return -1;
    }
    double count_d = span / step;
    if (!(count_d >= 0.0)) count_d = 0.0;
    if (!(count_d <= (double)(capacity - 4))) {
        PyErr_SetString(PyExc_ValueError, "state grid exceeds the supported size");
        return -1;
    }
    int count = (int)count_d + 1;
    if (count < 1) count = 1;

    int m = 0;
    for (int i = 0; i < count; i++) {
        if (m >= capacity) {
            PyErr_SetString(PyExc_ValueError, "state grid exceeds the supported size");
            return -1;
        }
        out[m++] = e_min + (double)i * step;
    }
    if (m >= capacity) {
        PyErr_SetString(PyExc_ValueError, "state grid exceeds the supported size");
        return -1;
    }
    if (out[m - 1] < e_max - EPS_KWH) out[m++] = e_max;
    for (int a = 0; a < n_anchors; a++) {
        double value = clampd(anchors[a], e_min, e_max);
        int duplicate = 0;
        for (int k = 0; k < m; k++) {
            if (fabs(value - out[k]) <= 1e-6) {
                duplicate = 1;
                break;
            }
        }
        if (duplicate) continue;
        if (m >= capacity) {
            PyErr_SetString(PyExc_ValueError, "state grid exceeds the supported size");
            return -1;
        }
        out[m++] = value;
    }
    insertion_sort(out, m);
    *out_n = m;
    *out_step = step;
    return 0;
}

/* soc_window_kw: the SoC-safe [lo, hi] for one state. */
static void soc_window(const Config *c, double e, double dt, double *lo_out, double *hi_out) {
    double lo = -c->p_discharge_kw;
    double hi = c->p_charge_kw;
    if (dt > 0.0 && c->eta_charge > 0.0 && c->eta_discharge > 0.0) {
        double room = c->e_max_kwh - e;
        if (room < 0.0) room = 0.0;
        double reachable_hi = room / (dt * c->eta_charge);
        if (reachable_hi < hi) hi = reachable_hi;
        double above = e - c->e_min_kwh;
        if (above < 0.0) above = 0.0;
        double reachable_lo = (-above * c->eta_discharge) / dt;
        if (reachable_lo > lo) lo = reachable_lo;
    }
    if (lo > hi) lo = hi;
    *lo_out = lo;
    *hi_out = hi;
}

/* grid_window_kw: the grid-feasible [lo, hi] for one slot (state independent). */
static void grid_window(const Config *c, double base, double sell, double buy, double *lo_out,
                        double *hi_out) {
    double lo = -c->p_discharge_kw;
    double hi = c->p_charge_kw;
    if (c->max_import_kw > 0.0) {
        double cap = c->max_import_kw - base;
        if (cap < hi) hi = cap;
    }
    if (c->max_export_kw > 0.0) {
        double cap = -(c->max_export_kw + base);
        if (cap > lo) lo = cap;
    }
    /* Export bounds, in the Python reference's precedence (D2): the user's allow_export is
     * strictest, then the contract refusal (forced PV surplus still exports), then the v1
     * zero/unknown-price rule. */
    if (!c->allow_export) {
        if (-base > lo) lo = -base;
    } else if (c->refuse_battery_export) {
        double cap = base > 0.0 ? -base : 0.0;
        if (cap > lo) lo = cap;
    } else if (!(sell > 0.0)) {
        if (-base > lo) lo = -base;
    }
    if (!c->allow_grid_charge) {
        double cap = -base;
        if (cap < 0.0) cap = 0.0;
        if (cap < hi) hi = cap;
    }
    if (c->has_sell_floor) {
        if (sell < c->sell_floor_eur_per_kwh) {
            double cap = base > 0.0 ? -base : 0.0;
            if (cap > lo) lo = cap;
        }
        if (base > 0.0 && buy < c->sell_floor_eur_per_kwh) {
            if (lo < 0.0) lo = 0.0;
        }
    }
    *lo_out = lo;
    *hi_out = hi;
}

static void make_ctx(const Config *c, SlotCtx *ctx, double pv, double load, double buy, double sell,
                     double minutes) {
    ctx->dt = minutes / 60.0;
    ctx->buy = buy;
    ctx->sell = sell;
    ctx->base = load - pv;
    ctx->surplus = pv - load;
    if (ctx->surplus < 0.0) ctx->surplus = 0.0;
    if (c->pv_preference == PV_PREFER) ctx->pv_price = 0.0;
    else if (c->pv_preference == PV_AVOID) ctx->pv_price = buy;
    else ctx->pv_price = sell;
    grid_window(c, ctx->base, sell, buy, &ctx->grid_lo, &ctx->grid_hi);
}

/* power_bounds_kw: the intersected window, plus the two halves for the caller's bookkeeping. */
static void bounds(const Config *c, const SlotCtx *ctx, double e, double dt, double *lo_out,
                   double *hi_out, double *soc_lo_out, double *soc_hi_out) {
    double soc_lo, soc_hi;
    soc_window(c, e, dt, &soc_lo, &soc_hi);
    double lo = soc_lo > ctx->grid_lo ? soc_lo : ctx->grid_lo;
    double hi = soc_hi < ctx->grid_hi ? soc_hi : ctx->grid_hi;
    if (lo > hi) {
        double middle = (ctx->grid_lo + ctx->grid_hi) / 2.0;
        lo = hi = clampd(middle, soc_lo, soc_hi);
    }
    if (hi < lo) lo = hi = clampd(0.0, soc_lo, soc_hi);
    *lo_out = lo;
    *hi_out = hi;
    *soc_lo_out = soc_lo;
    *soc_hi_out = soc_hi;
}

/* _actions: evenly spaced powers in [lo, hi], 0 always when feasible, sorted and deduped.
 *
 * ``align`` adds the two grid-neutral powers the planner cares about: charging exactly the PV
 * surplus (grid flows neither way) and serving exactly the house (grid flows neither way).  Without
 * them a 24-step lattice can only round a 0.10 kW surplus to 0.24 kW (importing the difference) or
 * spill a 0.13 kW discharge to the grid.
 *
 * Returns the count, or -1 when the caller's capacity is too small (never a partial write). */
static int build_actions(double lo, double hi, long steps_in, int has_extra, double extra,
                         int align, double surplus, double house, double *out, long capacity) {
    long steps = steps_in;
    if (steps < 1) steps = 1;
    if (steps + 4 > capacity) return -1;
    int n = 0;
    if (hi - lo <= EPS_KW) {
        out[n++] = clampd(0.0, lo, hi);
        return n;
    }
    if (steps == 1) {
        out[n++] = (lo + hi) / 2.0;
    } else {
        for (long j = 0; j < steps; j++) {
            out[n++] = lo + (hi - lo) * (double)j / (double)(steps - 1);
        }
    }
    out[n++] = clampd(0.0, lo, hi);
    if (has_extra && lo - EPS_KW <= extra && extra <= hi + EPS_KW) {
        out[n++] = clampd(extra, lo, hi);
    }
    if (align) {
        if (lo - EPS_KW <= surplus && surplus <= hi + EPS_KW) {
            out[n++] = clampd(surplus, lo, hi);
        }
        if (lo - EPS_KW <= house && house <= hi + EPS_KW) {
            out[n++] = clampd(house, lo, hi);
        }
    }
    insertion_sort(out, n);
    int w = 0;
    for (int k = 0; k < n; k++) {
        if (w == 0 || fabs(out[k] - out[w - 1]) > 1e-9) out[w++] = out[k];
    }
    return w;
}

/* _interp: value of the next slot's value function at the exact reachable energy.
 *
 * ``grid`` is ``e_min + i*step`` apart from at most a few appended anchor/ceiling points, so the
 * reciprocal-step estimate lands within a couple of positions of ``bisect_right`` and the two
 * adjust loops finish in O(1) instead of a binary search.  The estimate is only a hint: the loops
 * pick the exact ``bisect_right`` index.  The estimate is clamped in the double domain before the
 * cast, so a NaN/overflowing energy can never reach an undefined int conversion. */
static double interp(const double *values, const double *grid, int m, double inv_step, double e) {
    if (m == 0) return 0.0;
    if (!(e > grid[0])) return values[0];
    if (e >= grid[m - 1]) return values[m - 1];
    double estimate = (e - grid[0]) * inv_step;
    int i;
    if (!(estimate > 0.0)) i = 0;
    else if (estimate >= (double)(m - 2)) i = m - 2;
    else i = (int)estimate;
    while (i > 0 && grid[i] > e) i--;
    while (i < m - 2 && grid[i + 1] <= e) i++;
    double span = grid[i + 1] - grid[i];
    if (span <= 0.0) return values[i];
    return values[i] + (e - grid[i]) / span * (values[i + 1] - values[i]);
}

static double next_energy(const Config *c, double e, double p, double dt) {
    if (p >= 0.0) return e + dt * p * c->eta_charge;
    return e + dt * p / c->eta_discharge;
}

static double wear_cost(const Config *c, double e_from, double e_to) {
    double drop = e_from - e_to;
    if (drop < 0.0) drop = 0.0;
    return c->wear_rate * drop;
}

static double exact_power(const Config *c, double e, double e_target, double dt) {
    double delta = e_target - e;
    if (delta >= 0.0) return dt > 0.0 ? delta / (dt * c->eta_charge) : 0.0;
    return dt > 0.0 ? delta * c->eta_discharge / dt : 0.0;
}

/* slot_cost_eur at battery power p (positive = charging). */
static double slot_cost(const Config *c, const SlotCtx *ctx, double dt, double p) {
    double grid = ctx->base + p;
    double imported = grid > 0.0 ? grid : 0.0;
    double exported = grid < 0.0 ? -grid : 0.0;
    double cost = dt * (ctx->buy * imported - ctx->sell * exported);
    if (p > 0.0) {
        double pv_kwh = p < ctx->surplus ? p : ctx->surplus;
        double pv_price_delta = ctx->pv_price - ctx->sell;
        if (pv_kwh > 0.0 && pv_price_delta != 0.0) cost += dt * pv_price_delta * pv_kwh;
        if (c->min_spread_eur_per_kwh > 0.0) {
            double from_grid = p - ctx->surplus;
            if (from_grid < 0.0) from_grid = 0.0;
            cost += c->min_spread_eur_per_kwh * from_grid * dt;
        }
    }
    return cost;
}

/* best_at for an off-grid energy during reconstruction (the reference re-solves it, and does not
 * add to the iteration count). */
static double choose_power(const Config *c, const SlotCtx *ctx, const double *grid, int m,
                           double inv_step, const double *next_value, double e_now, double dt,
                           int last_slot, double *actions, long actions_capacity) {
    double lo, hi, soc_lo, soc_hi;
    bounds(c, ctx, e_now, dt, &lo, &hi, &soc_lo, &soc_hi);
    int has_extra = last_slot && c->has_target;
    double extra = has_extra ? exact_power(c, e_now, c->e_target, dt) : 0.0;
    int na = build_actions(lo, hi, c->power_steps, has_extra, extra, c->align_actions,
                           ctx->surplus, -ctx->base, actions, actions_capacity);
    if (na < 0) return clampd(0.0, lo, hi);
    double best = INFINITY;
    double best_p = clampd(0.0, lo, hi);
    for (int j = 0; j < na; j++) {
        double p = actions[j];
        double e_next = next_energy(c, e_now, p, dt);
        double cost = slot_cost(c, ctx, dt, p);
        cost += wear_cost(c, e_now, e_next);
        cost += interp(next_value, grid, m, inv_step, e_next);
        if (cost < best) {
            best = cost;
            best_p = p;
        }
    }
    return best_p;
}

/* The value iteration + path reconstruction.  No Python object is touched below this line and the
 * GIL is released around it; every buffer arrives pre-sized and every capacity was checked. */
static int run_solve(const Config *c, const double *pv, const double *load, const double *buy,
                     const double *sell, const double *minutes, int n, const double *grid, int m,
                     double inv_step, const double *terminal_row, double *value, double *argmin,
                     double *actions, long actions_capacity, double *energies, double *powers,
                     double *windows, long *iterations_out, double *terminal_error_out,
                     double *cycle_cost_out, double *objective_out, double *internal_out) {
    /* Terminal condition: the caller's stage-3 row verbatim when one was supplied (the row
     * already carries the target penalty), otherwise the pre-stage-3 salvage row. */
    double *terminal = value + (size_t)n * (size_t)m;
    for (int k = 0; k < m; k++) {
        if (terminal_row != NULL) {
            terminal[k] = terminal_row[k];
            continue;
        }
        double v = -c->salvage * grid[k];
        if (c->has_target) v += c->target_penalty * fabs(grid[k] - c->e_target);
        terminal[k] = v;
    }

    long iterations = 0;
    for (int i = n - 1; i >= 0; i--) {
        SlotCtx ctx;
        make_ctx(c, &ctx, pv[i], load[i], buy[i], sell[i], minutes[i]);
        double dt = ctx.dt;
        double *row = value + (size_t)i * (size_t)m;
        double *next_row = value + (size_t)(i + 1) * (size_t)m;
        double *choices = argmin + (size_t)i * (size_t)m;
        int last_slot = (i == n - 1);
        for (int k = 0; k < m; k++) {
            double e = grid[k];
            double lo, hi, soc_lo, soc_hi;
            bounds(c, &ctx, e, dt, &lo, &hi, &soc_lo, &soc_hi);
            int has_extra = last_slot && c->has_target;
            double extra = has_extra ? exact_power(c, e, c->e_target, dt) : 0.0;
            int na = build_actions(lo, hi, c->power_steps, has_extra, extra, c->align_actions,
                                   ctx.surplus, -ctx.base, actions, actions_capacity);
            if (na < 0) return -1;
            double best = INFINITY;
            double best_p = clampd(0.0, lo, hi);
            for (int j = 0; j < na; j++) {
                double p = actions[j];
                double e_next = next_energy(c, e, p, dt);
                double cost = slot_cost(c, &ctx, dt, p);
                cost += wear_cost(c, e, e_next);
                cost += interp(next_row, grid, m, inv_step, e_next);
                iterations++;
                if (cost < best) { /* strict: ties resolve to the lowest power */
                    best = cost;
                    best_p = p;
                }
            }
            row[k] = best;
            choices[k] = best_p;
        }
    }

    double e_now = clampd(c->start_kwh, c->e_min_kwh, c->e_max_kwh);
    double cycle_cost = 0.0;
    double objective = 0.0;
    for (int i = 0; i < n; i++) {
        SlotCtx ctx;
        make_ctx(c, &ctx, pv[i], load[i], buy[i], sell[i], minutes[i]);
        double dt = ctx.dt;
        double lo, hi, soc_lo, soc_hi;
        bounds(c, &ctx, e_now, dt, &lo, &hi, &soc_lo, &soc_hi);
        int found = -1;
        for (int k = 0; k < m; k++) {
            if (fabs(grid[k] - e_now) <= 1e-9) {
                found = k;
                break;
            }
        }
        double chosen = found >= 0
                            ? argmin[(size_t)i * (size_t)m + (size_t)found]
                            : choose_power(c, &ctx, grid, m, inv_step,
                                           value + (size_t)(i + 1) * (size_t)m, e_now, dt,
                                           i == n - 1, actions, actions_capacity);
        double p = clampd(chosen, lo, hi);
        energies[i] = e_now;
        powers[i] = p;
        windows[(size_t)4 * (size_t)i + 0] = soc_lo;
        windows[(size_t)4 * (size_t)i + 1] = soc_hi;
        windows[(size_t)4 * (size_t)i + 2] = ctx.grid_lo;
        windows[(size_t)4 * (size_t)i + 3] = ctx.grid_hi;
        double e_next = next_energy(c, e_now, p, dt);
        cycle_cost += wear_cost(c, e_now, e_next);
        objective += slot_cost(c, &ctx, dt, p);
        e_now = clampd(e_next, c->e_min_kwh, c->e_max_kwh);
    }
    if (terminal_row != NULL) {
        /* Stage 3: the row *is* the continuation value, so the path objective's terminal term is
         * the same interpolated row the last slot's sweep used -- no salvage and no penalty (the
         * penalty is already inside the supplied row). */
        objective += cycle_cost + interp(terminal_row, grid, m, inv_step, e_now);
    } else {
        objective += cycle_cost - c->salvage * e_now;
    }
    double terminal_error = 0.0;
    if (c->has_target) {
        if (terminal_row == NULL) objective += c->target_penalty * fabs(e_now - c->e_target);
        terminal_error = fabs(e_now - c->e_target);
    }

    double start_clamped = clampd(c->start_kwh, c->e_min_kwh, c->e_max_kwh);
    int start_index = 0;
    double best_distance = fabs(grid[0] - start_clamped);
    for (int k = 1; k < m; k++) {
        double distance = fabs(grid[k] - start_clamped);
        if (distance < best_distance) {
            best_distance = distance;
            start_index = k;
        }
    }

    *iterations_out = iterations;
    *terminal_error_out = terminal_error;
    *cycle_cost_out = cycle_cost;
    *objective_out = objective;
    *internal_out = value[start_index];
    return 0;
}
/* Per-slot domain checks: reject any combination that could make a transition, a soc window or an
 * exact-power result non-finite (and therefore unsafe to interpolate or cast). */
static int validate_slots(const Config *c, const double *pv, const double *load,
                          const double *minutes, int n) {
    double span = c->e_max_kwh - c->e_min_kwh;
    for (int i = 0; i < n; i++) {
        double dt = minutes[i] / 60.0;
        if (!isfinite(dt) || !(dt >= 0.0)) {
            PyErr_Format(PyExc_ValueError, "minutes[%d] is not a finite non-negative duration", i);
            return -1;
        }
        double base = load[i] - pv[i];
        if (!isfinite(base) || !isfinite(c->max_import_kw - base) ||
            !isfinite(-(c->max_export_kw + base))) {
            PyErr_Format(PyExc_ValueError, "slot %d produces a non-finite grid flow", i);
            return -1;
        }
        if (dt <= 0.0) continue;
        double charge_factor = dt * c->eta_charge;
        double discharge_factor = dt / c->eta_discharge;
        double charge_move = dt * c->p_charge_kw * c->eta_charge;
        double discharge_move = dt * c->p_discharge_kw / c->eta_discharge;
        if (!isfinite(charge_factor) || !(charge_factor > 0.0) || !isfinite(discharge_factor) ||
            !(discharge_factor > 0.0)) {
            PyErr_Format(PyExc_ValueError, "slot %d makes the battery transition factors non-finite",
                         i);
            return -1;
        }
        if (!isfinite(charge_move) || !isfinite(discharge_move) ||
            !isfinite(c->e_max_kwh + charge_move) || !isfinite(c->e_min_kwh - discharge_move)) {
            PyErr_Format(PyExc_ValueError, "slot %d makes the battery transitions non-finite", i);
            return -1;
        }
        if (!isfinite(span / charge_factor) || !isfinite(span / discharge_factor) ||
            !isfinite(span * c->eta_discharge / dt) || !isfinite(span / (dt * c->eta_charge))) {
            PyErr_Format(PyExc_ValueError, "slot %d makes the soc window non-finite", i);
            return -1;
        }
        if (c->has_target) {
            double reach = c->e_target - c->e_min_kwh;
            if (!isfinite(reach / (dt * c->eta_charge)) ||
                !isfinite(reach * c->eta_discharge / dt)) {
                PyErr_Format(PyExc_ValueError, "slot %d makes the terminal target non-finite", i);
                return -1;
            }
        }
    }
    return 0;
}
