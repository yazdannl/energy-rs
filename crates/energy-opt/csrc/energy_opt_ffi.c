/* The plain C ABI of the DP solver: flat double arrays + one fixed parameter block -> one
 * result, plus a free function.  No Python, no C++, no callbacks.
 *
 * Everything that decides a plan lives in csrc/core_solver.c (generated verbatim from the old
 * add-on's `_core.c`) behind the bridge in csrc/core_adapter.c.  This file owns only what the
 * CPython entry point used to do around it -- argument and finiteness checks, the state grid and
 * its complexity coarsening, allocation, and the return status -- because that part had to change
 * anyway (a dict became a struct).
 *
 * Invariants:
 *   * every buffer belongs to one allocation owned by the result, so `eo_free` cannot leak;
 *   * every size is computed with checked arithmetic, and every array is validated finite before
 *     a single numeric expression runs (upstream's `to_doubles` rule);
 *   * failures leave `out` zeroed and a message in `eo_last_error()`;
 *   * the error buffers are thread-local, and no other state is shared between calls.
 */

#include "core_solver.h"

static _Thread_local char eo_error[256];

static void set_error(const char *message) {
    size_t n = 0;
    if (message == NULL) message = "";
    while (message[n] != '\0' && n + 1 < sizeof(eo_error)) n++;
    if (n > 0) memcpy(eo_error, message, n);
    eo_error[n] = '\0';
}

const char *eo_last_error(void) { return eo_error[0] != '\0' ? eo_error : "ok"; }

static int all_finite(const double *values, size_t count, const char *message) {
    for (size_t i = 0; i < count; i++) {
        if (!isfinite(values[i])) {
            set_error(message);
            return -1;
        }
    }
    return 0;
}

static int checked_mul_size(size_t a, size_t b, size_t *out) {
    if (a != 0 && b > (size_t)-1 / a) return -1;
    *out = a * b;
    return 0;
}

/* The one allocation a result owns: a double arena inside the owner block. */
typedef struct {
    eo_result result;
    double *arena;
} eo_owner;

void eo_free(eo_result *result) {
    if (result == NULL) return;
    eo_owner *owner = (eo_owner *)result->owner;
    if (owner != NULL) {
        free(owner->arena);
        free(owner);
    }
    memset(result, 0, sizeof(*result));
}

int eo_solve(const eo_core_params *params, int32_t n_slots, const double *pv_kw,
             const double *load_kw, const double *buy, const double *sell, const double *minutes,
             const double *terminal_row, int32_t terminal_row_len, eo_result *out) {
    if (out == NULL) {
        set_error("the result pointer is null");
        return EO_ERR_ARGUMENT;
    }
    memset(out, 0, sizeof(*out));
    set_error("ok");

    if (params == NULL || pv_kw == NULL || load_kw == NULL || buy == NULL || sell == NULL ||
        minutes == NULL) {
        set_error("an input array (or the parameter block) is null");
        return EO_ERR_ARGUMENT;
    }
    if (n_slots < 0) {
        set_error("the slot count is negative");
        return EO_ERR_ARGUMENT;
    }
    if ((long)n_slots > EO_MAX_SLOTS) {
        set_error("too many slots for the supported core");
        return EO_ERR_LIMITS;
    }
    if (terminal_row == NULL && terminal_row_len != 0) {
        set_error("a terminal row length was supplied without a row");
        return EO_ERR_ARGUMENT;
    }
    size_t n = (size_t)n_slots;

    if (all_finite(pv_kw, n, "pv_kw is not finite") < 0 ||
        all_finite(load_kw, n, "load_kw is not finite") < 0 ||
        all_finite(buy, n, "buy is not finite") < 0 ||
        all_finite(sell, n, "sell is not finite") < 0 ||
        all_finite(minutes, n, "minutes is not finite") < 0) {
        return EO_ERR_DOMAIN;
    }
    /* Non-finite or out-of-range parameters are the config's business: the bridge checks
     * finiteness and domain, and reports the reason through its own message slot. */
    if (eo_core_check_config(params) < 0) {
        set_error(eo_core_error_message());
        return EO_ERR_DOMAIN;
    }

    /* State grid, exactly as ``core_solve`` built it: the reference grid, then the complexity
     * coarsening when the state count exceeds ``MAX_ITERATIONS / (n * actions)``. */
    size_t grid_capacity = (size_t)params->max_states + 8;
    double anchors[2];
    int n_anchors = 0;
    anchors[n_anchors++] = params->start_kwh < params->e_min_kwh
                               ? params->e_min_kwh
                               : (params->start_kwh > params->e_max_kwh ? params->e_max_kwh
                                                                        : params->start_kwh);
    if (params->has_target != 0) anchors[n_anchors++] = params->e_target;

    double *grid = (double *)malloc(grid_capacity * sizeof(double));
    if (grid == NULL) {
        set_error("out of memory");
        return EO_ERR_MEMORY;
    }
    long states = 0;
    double step = 0.0;
    if (eo_core_build_grid(params, anchors, n_anchors, grid, (long)grid_capacity, &states,
                           &step) < 0) {
        set_error(eo_core_error_message());
        free(grid);
        return EO_ERR_GRID;
    }

    long action_count = params->power_steps + (params->align_actions != 0 ? 4 : 1);
    if (action_count < 2) action_count = 2;
    long budget = params->max_iterations / (n_slots > 0 ? (long)n_slots * action_count : 1);
    if (budget < 2) budget = 2;
    if (states > budget) {
        double span = params->e_max_kwh - params->e_min_kwh;
        if (span < 0.0) span = 0.0;
        double denom = (double)(budget - 1);
        if (denom < 1.0) denom = 1.0;
        double widened = span / denom;
        if (widened < step) widened = step;
        eo_core_params coarsened = *params;
        coarsened.soc_step_kwh = widened;
        if (eo_core_build_grid(&coarsened, anchors, n_anchors, grid, (long)grid_capacity, &states,
                               &step) < 0) {
            set_error(eo_core_error_message());
            free(grid);
            return EO_ERR_GRID;
        }
    }
    if (!(step > 0.0) || !isfinite(step)) {
        set_error("soc step is not usable for these bounds");
        free(grid);
        return EO_ERR_GRID;
    }

    /* A continuation row is only interpretable over the grid the sweep uses: a wrong length is
     * refused here rather than read out of bounds or silently ignored. */
    if (terminal_row != NULL) {
        if ((long)terminal_row_len != states) {
            set_error("the terminal row length does not match the state grid");
            free(grid);
            return EO_ERR_TERMINAL_ROW;
        }
        if (all_finite(terminal_row, (size_t)states, "terminal_row is not finite") < 0) {
            free(grid);
            return EO_ERR_DOMAIN;
        }
    }

    size_t m = (size_t)states;
    size_t rows = n + 1;
    size_t value_count = 0;
    size_t argmin_count = 0;
    size_t windows_count = 0;
    if (checked_mul_size(rows, m, &value_count) < 0 ||
        checked_mul_size(n, m, &argmin_count) < 0 ||
        checked_mul_size(n, 4, &windows_count) < 0) {
        set_error("scenario dimensions exceed the supported size");
        free(grid);
        return EO_ERR_LIMITS;
    }
    if (value_count > (size_t)INT32_MAX) {
        set_error("the value function exceeds the supported size");
        free(grid);
        return EO_ERR_LIMITS;
    }
    size_t actions_capacity = (size_t)params->power_steps + 4;
    if (actions_capacity < 5) actions_capacity = 5;
    size_t arena_elements = grid_capacity + value_count + argmin_count + actions_capacity +
                            2 * n + windows_count;
    size_t arena_bytes = 0;
    if (checked_mul_size(arena_elements, sizeof(double), &arena_bytes) < 0) {
        set_error("scenario dimensions exceed the supported size");
        free(grid);
        return EO_ERR_LIMITS;
    }

    eo_owner *owner = (eo_owner *)calloc(1, sizeof(eo_owner));
    if (owner == NULL) {
        set_error("out of memory");
        free(grid);
        return EO_ERR_MEMORY;
    }
    double *arena = (double *)malloc(arena_bytes > 0 ? arena_bytes : 1);
    if (arena == NULL) {
        set_error("out of memory");
        free(owner);
        free(grid);
        return EO_ERR_MEMORY;
    }

    /* Carve the arena: grid | value | argmin | actions | energies | powers | windows. */
    size_t offset = 0;
    double *grid_out = arena + offset;
    memcpy(grid_out, grid, m * sizeof(double));
    offset += grid_capacity;
    double *value = arena + offset;
    offset += value_count;
    double *argmin = arena + offset;
    offset += argmin_count;
    double *actions = arena + offset;
    offset += actions_capacity;
    double *energies = arena + offset;
    offset += n;
    double *powers = arena + offset;
    offset += n;
    double *windows = arena + offset;
    free(grid); /* the copy in the arena is what the result returns */

    long iterations = 0;
    double terminal_error = 0.0, cycle_cost = 0.0, objective = 0.0, internal = 0.0;
    if (eo_core_run(params, n_slots, pv_kw, load_kw, buy, sell, minutes, grid_out, states, step,
                    terminal_row, value, argmin, actions, (long)actions_capacity, energies, powers,
                    windows, &iterations, &terminal_error, &cycle_cost, &objective, &internal) !=
        0) {
        set_error(eo_core_error_message());
        free(arena);
        free(owner);
        return EO_ERR_SWEEP;
    }

    owner->arena = arena;
    owner->result.grid = grid_out;
    owner->result.grid_len = states;
    owner->result.value = value;
    owner->result.value_len = (long)value_count;
    owner->result.energies = energies;
    owner->result.energies_len = (long)n;
    owner->result.powers = powers;
    owner->result.powers_len = (long)n;
    owner->result.windows = windows;
    owner->result.windows_len = (long)windows_count;
    owner->result.effective_step_kwh = step;
    owner->result.objective_eur = objective;
    owner->result.cycle_cost_eur = cycle_cost;
    owner->result.terminal_error_kwh = terminal_error;
    owner->result.internal_objective_eur = internal;
    owner->result.iterations = iterations;
    owner->result.states = states;
    owner->result.owner = owner;
    *out = owner->result;
    return EO_OK;
}
