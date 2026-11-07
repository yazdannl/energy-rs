//! Contract tests for the crate's public surface: defaults, typed errors and degenerate scenarios.

use energy_opt::{
    BatterySpec, Config, ConfigError, InputError, PvChargePreference, SolveError, SolveInput,
    Window, solve, solve_with_terminal_row,
};

fn battery() -> BatterySpec {
    BatterySpec {
        e_min_kwh: 0.5,
        e_max_kwh: 9.5,
        p_charge_kw: 3.0,
        p_discharge_kw: 3.0,
        eta_charge: 0.95,
        eta_discharge: 0.95,
    }
}

#[test]
fn the_defaults_are_the_shipped_optimizer_defaults() {
    let config = Config::new(battery(), 4.0);
    assert_eq!(config.slot_minutes, 60.0);
    assert_eq!(config.soc_step_kwh, 0.02);
    assert_eq!(config.power_steps, 24);
    assert_eq!(config.max_states, 400);
    assert_eq!(config.max_iterations, 2_000_000);
    assert!(config.allow_grid_charge);
    assert!(config.allow_export);
    assert_eq!(config.salvage, 0.0);
    assert_eq!(config.e_target, None);
    assert_eq!(config.sell_floor_eur_per_kwh, None);
    assert!(!config.refuse_battery_export);
    assert_eq!(config.pv_charge_preference, PvChargePreference::Auto);
    config.validate().expect("the shipped defaults are valid");
}

#[test]
fn an_empty_scenario_solves_to_a_terminal_grid() {
    let config = Config::new(battery(), 4.0);
    let input = SolveInput::new(&[], &[], &[], &[], &[]);
    let result = solve(&input, &config).expect("an empty scenario is legal");
    assert!(result.powers.is_empty());
    assert!(result.energies.is_empty());
    assert_eq!(result.states, result.grid.len());
    assert!(result.states >= 8, "the core's minimum state count");
    assert_eq!(result.value.len(), result.states);
    assert_eq!(result.iterations, 0);
    assert_eq!(result.terminal_error_kwh, 0.0);
    assert_eq!(result.objective_eur, 0.0);
}

#[test]
fn mismatched_lengths_are_refused() {
    let config = Config::new(battery(), 4.0);
    let input = SolveInput::new(&[0.0, 0.0], &[0.0], &[0.2, 0.2], &[0.0, 0.0], &[60.0, 60.0]);
    match solve(&input, &config) {
        Err(SolveError::Input(InputError::Length {
            name,
            expected,
            got,
        })) => {
            assert_eq!(name, "load_kw");
            assert_eq!(expected, 2);
            assert_eq!(got, 1);
        }
        other => panic!("expected a length error, got {other:?}"),
    }
}

#[test]
fn non_finite_inputs_are_refused_before_the_solver_runs() {
    let config = Config::new(battery(), 4.0);
    let input = SolveInput::new(&[0.0], &[0.0], &[f64::NAN], &[0.0], &[60.0]);
    match solve(&input, &config) {
        Err(SolveError::Input(InputError::NonFinite { name, index })) => {
            assert_eq!(name, "buy");
            assert_eq!(index, 0);
        }
        other => panic!("expected a finiteness error, got {other:?}"),
    }

    let input = SolveInput::new(&[0.0], &[0.0], &[0.2], &[f64::INFINITY], &[60.0]);
    assert!(matches!(
        solve(&input, &config),
        Err(SolveError::Input(InputError::NonFinite {
            name: "sell",
            ..
        }))
    ));
}

#[test]
fn an_invalid_config_is_refused_with_a_typed_error() {
    let mut config = Config::new(battery(), 4.0);
    config.eta_charge = 0.0;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::Efficiencies { .. })
    ));
    let input = SolveInput::new(&[0.0], &[0.0], &[0.2], &[0.0], &[60.0]);
    assert!(matches!(
        solve(&input, &config),
        Err(SolveError::Config(ConfigError::Efficiencies { .. }))
    ));

    let mut config = Config::new(battery(), 4.0);
    config.max_states = 4;
    assert!(matches!(config.validate(), Err(ConfigError::MaxStates(4))));

    let mut config = Config::new(battery(), 4.0);
    config.e_max_kwh = 0.1;
    assert!(matches!(
        config.validate(),
        Err(ConfigError::EnergyBounds { .. })
    ));
}

#[test]
fn an_unusable_terminal_row_is_refused() {
    let config = Config::new(battery(), 4.0);
    let input = SolveInput::new(
        &[0.0, 0.0],
        &[0.0, 0.0],
        &[0.2, 0.2],
        &[0.0, 0.0],
        &[60.0, 60.0],
    );
    let (grid, _) = energy_opt::validate::reference_grid(&config, 2);

    let short = vec![0.0; grid.len() - 1];
    assert!(matches!(
        solve_with_terminal_row(&input, &config, &short),
        Err(SolveError::TerminalRow(_))
    ));

    let mut non_finite = vec![0.0; grid.len()];
    non_finite[grid.len() / 2] = f64::NAN;
    assert!(matches!(
        solve_with_terminal_row(&input, &config, &non_finite),
        Err(SolveError::TerminalRow(_))
    ));

    let row = vec![0.0; grid.len()];
    let result = solve_with_terminal_row(&input, &config, &row).expect("a usable row solves");
    assert_eq!(result.terminal_row(), row.as_slice());
}

#[test]
fn too_many_slots_are_refused() {
    let config = Config::new(battery(), 4.0);
    let slots = energy_opt::EO_MAX_SLOTS + 1;
    let pv = vec![0.0; slots];
    let load = vec![0.0; slots];
    let buy = vec![0.2; slots];
    let sell = vec![0.0; slots];
    let minutes = vec![60.0; slots];
    let input = SolveInput::new(&pv, &load, &buy, &sell, &minutes);
    assert!(matches!(
        solve(&input, &config),
        Err(SolveError::Input(InputError::TooManySlots(_)))
    ));
}

#[test]
fn solves_are_deterministic_and_the_windows_keep_their_order() {
    let config = Config::new(battery(), 4.0);
    let input = SolveInput::new(
        &[0.0, 1.0, 2.0],
        &[1.0, 0.5, 0.2],
        &[0.30, 0.10, 0.40],
        &[0.05, 0.05, 0.05],
        &[60.0, 60.0, 60.0],
    );
    let first = solve(&input, &config).expect("solve");
    let second = solve(&input, &config).expect("solve");
    assert_eq!(first, second);
    let window: Window = first.windows[0];
    assert_eq!(
        window.values(),
        [
            window.soc_lo_kw,
            window.soc_hi_kw,
            window.grid_lo_kw,
            window.grid_hi_kw
        ]
    );
    assert!(window.soc_lo_kw <= window.soc_hi_kw);
    assert!(window.grid_lo_kw <= window.grid_hi_kw);
}

#[test]
fn the_core_identity_is_reported() {
    assert_eq!(energy_opt::CORE_SOURCE_SHA256.len(), 64);
    assert_eq!(energy_opt::CORE_BUILD_SHA.len(), 16);
    assert_eq!(
        energy_opt::CORE_SOURCE_SHA256[..16],
        *energy_opt::CORE_BUILD_SHA
    );
    assert_eq!(energy_opt::CORE_VERSION, "ha-energy-manager 0.38.8");
}
