/* The C ABI around the extracted DP core (see csrc/UPSTREAM.md).
 *
 * Two layers live here:
 *
 *   1. the shim the extracted text needs (it is a CPython extension and calls ``PyErr_SetString``
 *      twice, in ``build_grid``), and
 *   2. ``eo_*`` -- the plain C entry points Rust calls.  Flat ``double`` arrays, one fixed
 *      parameter block, one result struct, one free function.  No Python, no allocation by the
 *      caller, no C++ types.
 *
 * Nothing in this header is part of the upstream source; the vendored ranges themselves are
 * byte-identical to ``_core.c`` (``scripts/extract_core.py --check``).
 */
#ifndef ENERGY_OPT_CORE_SOLVER_H
#define ENERGY_OPT_CORE_SOLVER_H

#include <math.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ---- the CPython shim ------------------------------------------------------------------- */

/* The upstream build reports its (never seen in production) domain failures through the CPython
 * error indicator.  The C ABI has no such thing, so those call sites are mapped onto the bridge's
 * one-slot error buffer, which ``eo_core_error_message`` returns.  Defining these three names is
 * the whole shim: the extracted lines are not edited. */
#define PyExc_ValueError NULL
#define PyErr_SetString(exception, message) eo_core_note_error(message)
#define PyErr_Format(exception, format, ...) eo_core_format_error(format, __VA_ARGS__)

void eo_core_note_error(const char *message);
void eo_core_format_error(const char *format, ...);
const char *eo_core_error_message(void);

/* The slot ceiling of the C ABI: the upstream ``CORE_MAX_DIMENSION``.  ``core_adapter.c``
 * static-asserts that the two are equal, so this can never drift from the extracted core. */
#define EO_MAX_SLOTS 1000000L

/* ---- the parameter block ---------------------------------------------------------------- */

/* A flat, fixed mirror of the upstream ``Config`` struct (field for field, same order).  The Rust
 * ``EoCoreParams`` is a ``#[repr(C)]`` copy of this, so the two layouts are checked by the
 * compiler on the Rust side and by `tests::abi` (size and offsets) at test time. */
typedef struct {
    double e_min_kwh;
    double e_max_kwh;
    double p_charge_kw;
    double p_discharge_kw;
    double eta_charge;
    double eta_discharge;
    double start_kwh;
    /*: Read only when ``has_target``. */
    double e_target;
    double slot_minutes;
    double soc_step_kwh;
    long power_steps;
    long max_states;
    long max_iterations;
    double max_import_kw;
    double max_export_kw;
    double min_spread_eur_per_kwh;
    /*: Read only when ``has_sell_floor``. */
    double sell_floor_eur_per_kwh;
    double wear_eur_per_kwh;
    double salvage;
    double target_penalty;
    int32_t has_target;
    int32_t allow_grid_charge;
    int32_t allow_export;
    int32_t refuse_battery_export;
    int32_t has_sell_floor;
    /*: 0 = auto, 1 = prefer_pv, 2 = avoid_pv (``PV_AUTO``/``PV_PREFER``/``PV_AVOID``). */
    int32_t pv_preference;
} eo_core_params;

/* Bridge entry points implemented by csrc/core_adapter.c, which includes core_solver.c so the
 * extracted ``static`` helpers stay untouched.  They return 0 on success and -1 on failure, with
 * the reason in ``eo_core_error_message()``. */

/* Config domain checks (a transcription of the upstream ``parse_config`` tail, ``_core.c:640``). */
int eo_core_check_config(const eo_core_params *params);

/* Per-slot domain checks (upstream ``validate_slots``). */
int eo_core_check_slots(const eo_core_params *params, int32_t n_slots, const double *pv_kw,
                        const double *load_kw, const double *minutes);

/* The reference state grid (upstream ``build_grid``) over a caller-provided buffer. */
int eo_core_build_grid(const eo_core_params *params, const double *anchors, int32_t n_anchors,
                       double *grid, long capacity, long *out_len, double *out_step);

/* The value iteration + path reconstruction (upstream ``run_solve``) over pre-sized buffers. */
int eo_core_run(const eo_core_params *params, int32_t n_slots, const double *pv_kw,
                const double *load_kw, const double *buy, const double *sell, const double *minutes,
                const double *grid, long states, double step, const double *terminal_row,
                double *value, double *argmin, double *actions, long actions_capacity,
                double *energies, double *powers, double *windows, long *iterations_out,
                double *terminal_error_out, double *cycle_cost_out, double *objective_out,
                double *internal_objective_out);

/* ---- the public C ABI ------------------------------------------------------------------- */

/* Status codes.  Every failure also leaves a message in ``eo_last_error()``. */
enum {
    EO_OK = 0,
    /*: A null pointer, a negative slot count or unequal array lengths. */
    EO_ERR_ARGUMENT = -1,
    /*: A dimension past ``CORE_MAX_DIMENSION`` / below ``CORE_MIN_STATES``. */
    EO_ERR_LIMITS = -2,
    /*: A non-finite or otherwise unusable number in the parameter block or the arrays. */
    EO_ERR_DOMAIN = -3,
    /*: The state grid could not be built. */
    EO_ERR_GRID = -4,
    /*: Allocation failed. */
    EO_ERR_MEMORY = -5,
    /*: The sweep failed after the grid was accepted. */
    EO_ERR_SWEEP = -6,
    /*: A terminal row was supplied with the wrong length. */
    EO_ERR_TERMINAL_ROW = -7
};

/* One solve's output.  Every pointer belongs to the result block and stays valid until
 * ``eo_free``; a caller never frees an individual array. */
typedef struct {
    /*: ``states`` grid points, ascending. */
    const double *grid;
    long grid_len;
    /*: ``(n_slots + 1) * states`` native doubles, row-major; row ``n`` is the terminal row. */
    const double *value;
    long value_len;
    const double *energies;
    long energies_len;
    const double *powers;
    long powers_len;
    /*: ``4 * n_slots``: soc_lo, soc_hi, grid_lo, grid_hi per slot. */
    const double *windows;
    long windows_len;
    double effective_step_kwh;
    double objective_eur;
    double cycle_cost_eur;
    double terminal_error_kwh;
    double internal_objective_eur;
    long iterations;
    long states;
    /*: Opaque owner block; internal to ``eo_free``. */
    void *owner;
} eo_result;

/* Run one solve.
 *
 * ``n_slots`` is the length of all five input arrays.  ``terminal_row`` is NULL for the salvage
 * model (with ``terminal_row_len`` 0), or a row of exactly ``states`` finite values (the stage-3
 * continuation row).  ``out`` is zeroed on failure and owns its buffers on success.
 */
int eo_solve(const eo_core_params *params, int32_t n_slots, const double *pv_kw,
             const double *load_kw, const double *buy, const double *sell, const double *minutes,
             const double *terminal_row, int32_t terminal_row_len, eo_result *out);

/* Release a result's buffers.  Safe on a zeroed struct and idempotent for one successful solve. */
void eo_free(eo_result *result);

/* Why the last call on this thread failed; "ok" after a success. */
const char *eo_last_error(void);

/* The SHA-256 prefix ``-DCORE_BUILD_SHA`` compiled into this artifact ("" when it was omitted).
 * Rust compares it with the digest of the source it shipped, so a stale object file cannot pass. */
const char *eo_build_sha(void);

#endif /* ENERGY_OPT_CORE_SOLVER_H */
