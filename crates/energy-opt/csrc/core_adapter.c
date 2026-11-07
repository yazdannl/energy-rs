/* The bridge between the extracted (``static``) numeric core and the flat C ABI.
 *
 * ``core_solver.c`` is generated verbatim from the old add-on's ``_core.c``, so its helpers are
 * ``static`` and its ``Config`` struct is internal.  This file includes it -- one translation
 * unit, no copies of the numerics -- and exposes exactly what ``energy_opt_ffi.c`` needs:
 *
 *   * the config domain checks that upstream ran inside ``parse_config`` (they are part of the
 *     contract, not of the marshalling), and
 *   * thin wrappers over ``build_grid``, ``validate_slots`` and ``run_solve`` that map the flat
 *     parameter block onto ``Config``.
 *
 * Nothing here computes with energy, price or SoC numbers; every arithmetic expression that
 * decides a plan stays in the extracted lines.
 */

#include "core_solver.c"

/* The shim target: the extracted ``build_grid`` reports its (unreachable in practice) failures
 * through ``PyErr_SetString``; that lands here instead of in a Python interpreter. */
/* The ABI slot cap must be the core's own dimension cap. */
_Static_assert(EO_MAX_SLOTS == (long)CORE_MAX_DIMENSION, "EO_MAX_SLOTS must mirror the core");

static _Thread_local char eo_note[256];

void eo_core_note_error(const char *message) {
    size_t n = 0;
    if (message == NULL) message = "";
    while (message[n] != '\0' && n + 1 < sizeof(eo_note)) n++;
    if (n > 0) memcpy(eo_note, message, n);
    eo_note[n] = '\0';
}

const char *eo_core_error_message(void) { return eo_note[0] != '\0' ? eo_note : "ok"; }

void eo_core_format_error(const char *format, ...) {
    va_list args;
    va_start(args, format);
    vsnprintf(eo_note, sizeof(eo_note), format, args);
    va_end(args);
}

/* The flat block -> Config, field for field (see the struct in core_solver.h). */
static void map_params(const eo_core_params *p, Config *c) {
    c->e_min_kwh = p->e_min_kwh;
    c->e_max_kwh = p->e_max_kwh;
    c->p_charge_kw = p->p_charge_kw;
    c->p_discharge_kw = p->p_discharge_kw;
    c->eta_charge = p->eta_charge;
    c->eta_discharge = p->eta_discharge;
    c->start_kwh = p->start_kwh;
    c->has_target = p->has_target != 0;
    c->e_target = p->e_target;
    c->slot_minutes = p->slot_minutes;
    c->soc_step_kwh = p->soc_step_kwh;
    c->power_steps = p->power_steps;
    c->allow_grid_charge = p->allow_grid_charge != 0;
    c->allow_export = p->allow_export != 0;
    c->refuse_battery_export = p->refuse_battery_export != 0;
    c->max_import_kw = p->max_import_kw;
    c->max_export_kw = p->max_export_kw;
    c->min_spread_eur_per_kwh = p->min_spread_eur_per_kwh;
    c->has_sell_floor = p->has_sell_floor != 0;
    c->sell_floor_eur_per_kwh = p->sell_floor_eur_per_kwh;
    /* Upstream: anything else means ``auto`` (``strcmp`` fallback in parse_config). */
    c->pv_preference = p->pv_preference == PV_PREFER
                           ? PV_PREFER
                           : (p->pv_preference == PV_AVOID ? PV_AVOID : PV_AUTO);
    c->wear_eur_per_kwh = p->wear_eur_per_kwh;
    c->wear_rate = c->wear_eur_per_kwh * c->eta_discharge;
    c->salvage = p->salvage;
    c->target_penalty = p->target_penalty;
    c->max_states = p->max_states;
    c->max_iterations = p->max_iterations;
}

/* ``parse_config``'s domain checks, without the dict (``_core.c:630-658``).  Any value that could
 * make a transition, a window or an exact-power result non-finite is refused here, before any
 * integer narrowing or allocation. */
int eo_core_check_config(const eo_core_params *p) {
    if (p == NULL) {
        eo_core_note_error("config is missing");
        return -1;
    }
    Config c;
    map_params(p, &c);
    if (!isfinite(c.e_min_kwh) || !isfinite(c.e_max_kwh) || !isfinite(c.p_charge_kw) ||
        !isfinite(c.p_discharge_kw) || !isfinite(c.eta_charge) || !isfinite(c.eta_discharge) ||
        !isfinite(c.start_kwh) || !isfinite(c.slot_minutes) || !isfinite(c.soc_step_kwh) ||
        !isfinite(c.max_import_kw) || !isfinite(c.max_export_kw) ||
        !isfinite(c.min_spread_eur_per_kwh) || !isfinite(c.wear_eur_per_kwh) ||
        !isfinite(c.salvage) || !isfinite(c.target_penalty)) {
        eo_core_note_error("a config value is not finite");
        return -1;
    }
    if (c.has_target && !isfinite(c.e_target)) {
        eo_core_note_error("e_target is not finite");
        return -1;
    }
    if (c.has_sell_floor && !isfinite(c.sell_floor_eur_per_kwh)) {
        eo_core_note_error("sell_floor_eur_per_kwh is not finite");
        return -1;
    }
    if (c.power_steps < 0 || c.power_steps > CORE_MAX_DIMENSION) {
        eo_core_note_error("power_steps is outside the supported range");
        return -1;
    }
    if (c.max_states < CORE_MIN_STATES || c.max_states > CORE_MAX_DIMENSION) {
        eo_core_note_error("max_states is outside the supported range");
        return -1;
    }
    if (c.max_iterations < 1) {
        eo_core_note_error("max_iterations must be positive");
        return -1;
    }
    if (!(c.e_min_kwh <= c.e_max_kwh) || !isfinite(c.e_max_kwh - c.e_min_kwh)) {
        eo_core_note_error("soc bounds do not produce a finite span");
        return -1;
    }
    if (!(c.eta_charge > 0.0) || !(c.eta_discharge > 0.0)) {
        eo_core_note_error("battery efficiencies must be positive");
        return -1;
    }
    if (!(c.p_charge_kw >= 0.0) || !(c.p_discharge_kw >= 0.0)) {
        eo_core_note_error("battery power limits must be non-negative");
        return -1;
    }
    if (!(c.slot_minutes >= 0.0)) {
        eo_core_note_error("slot_minutes must be non-negative");
        return -1;
    }
    return 0;
}

int eo_core_check_slots(const eo_core_params *params, int32_t n_slots, const double *pv_kw,
                        const double *load_kw, const double *minutes) {
    Config c;
    map_params(params, &c);
    return validate_slots(&c, pv_kw, load_kw, minutes, n_slots);
}

int eo_core_build_grid(const eo_core_params *params, const double *anchors, int32_t n_anchors,
                       double *grid, long capacity, long *out_len, double *out_step) {
    Config c;
    map_params(params, &c);
    int len = 0;
    double step = 0.0;
    if (build_grid(c.e_min_kwh, c.e_max_kwh, c.soc_step_kwh, c.max_states, anchors, n_anchors,
                   grid, (int)capacity, &len, &step) < 0) {
        return -1;
    }
    *out_len = (long)len;
    *out_step = step;
    return 0;
}

int eo_core_run(const eo_core_params *params, int32_t n_slots, const double *pv_kw,
                const double *load_kw, const double *buy, const double *sell, const double *minutes,
                const double *grid, long states, double step, const double *terminal_row,
                double *value, double *argmin, double *actions, long actions_capacity,
                double *energies, double *powers, double *windows, long *iterations_out,
                double *terminal_error_out, double *cycle_cost_out, double *objective_out,
                double *internal_objective_out) {
    Config c;
    map_params(params, &c);
    return run_solve(&c, pv_kw, load_kw, buy, sell, minutes, n_slots, grid, (int)states, 1.0 / step,
                     terminal_row, value, argmin, actions, actions_capacity, energies, powers,
                     windows, iterations_out, terminal_error_out, cycle_cost_out, objective_out,
                     internal_objective_out);
}

/* The digest the build compiled in (``-DCORE_BUILD_SHA``; "" when the define was omitted).  The
 * extraction keeps upstream's `#ifndef` fallback, so this is defined either way. */
const char *eo_build_sha(void) { return CORE_BUILD_SHA; }
