//! Every pair at once (PLAN.md M7.9): imposed phases, the domain's errors and mass units.

use super::*;

/// The input of `pair` read off `state`, molar; `None` where it needs a quality and the state is a single phase.
#[cfg(feature = "fluids-all")]
fn input_of(pair: Pair, state: &State) -> Option<Input> {
    let m = Basis::Molar;
    let value = |var: Var| match var {
        Var::T => Some(state.t()),
        Var::P => Some(state.p()),
        Var::Q => state.quality(),
        Var::D => Some(state.rho(m)),
        Var::H => Some(state.h(m)),
        Var::S => Some(state.s(m)),
        Var::U => Some(state.u(m)),
    };
    let (x, y) = pair.vars();
    Input::new(pair, value(x)?, value(y)?, m).ok()
}

/// Whether `state` reproduces both values of `input`: T and ρ within 1e-9 of each, p within 1e-9 of it and 1e-12 of
/// ρRT (the terms it cancels from), Q within 1e-9, and h, s and u as the acceptance gate asks ([`reproduces_x`]).
#[cfg(feature = "fluids-all")]
fn reproduces(state: &State, input: Input) -> bool {
    let (m, r, t) = (Basis::Molar, state.gas_constant(), state.t());
    let holds = |var: Var, want: f64| match var {
        Var::T => (t - want).abs() <= 1e-9 * want,
        Var::P => (state.p() - want).abs() <= 1e-9 * want + 1e-12 * state.rho(m) * r * t,
        Var::Q => state.quality().is_some_and(|q| (q - want).abs() <= 1e-9),
        Var::D => (state.rho(m) - want).abs() <= 1e-9 * want,
        Var::H => reproduces_x(Caloric::H, r, t, want, state.h(m)),
        Var::S => reproduces_x(Caloric::S, r, t, want, state.s(m)),
        Var::U => reproduces_x(Caloric::U, r, t, want, state.u(m)),
    };
    let ((vx, vy), (x, y)) = (input.pair().vars(), input.values());
    holds(vx, x) && holds(vy, y)
}

/// Whether `state` is `truth`: the same phase, T and ρ within 1e-9 of the truth's, Q within 1e-9.
#[cfg(feature = "fluids-all")]
fn is_truth(state: &State, truth: &State) -> bool {
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs();
    let quality = match (state.quality(), truth.quality()) {
        (Some(a), Some(b)) => (a - b).abs() <= 1e-9,
        (a, b) => a == b,
    };
    state.phase() == truth.phase()
        && near(state.t(), truth.t())
        && near(state.rho(Basis::Molar), truth.rho(Basis::Molar))
        && quality
}

/// The states a flash of `input` under `opts` returns: its state, or each root of an `Ambiguous` taken by
/// `Nearest`; none for any other refusal.
#[cfg(feature = "fluids-all")]
fn states_of(fluid: &Fluid, input: Input, opts: FlashOptions, result: &Result<State, Error>) -> Vec<State> {
    match result {
        Ok(state) => vec![*state],
        Err(Error::Ambiguous { roots }) => roots
            .as_slice()
            .iter()
            .map(|&root| fluid.flash(input, &opts.with_roots(RootPolicy::Nearest(root))).unwrap())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// ROT-069 (map 03 §6 row 1): CoolProp's DQ, HQ and QS impose two-phase on the backend and never clear it, so its
/// reused Water state then gives PH at 30 MPa and h(700 K) 270.79 K and PT throws; after DQ, DT at 10 mol/m³ and
/// 700 K is two-phase; after PH under an imposed gas, PT at 1 bar and 300 K is a gas. Here a hint is an argument of
/// one call and a `Fluid` is immutable: on one `Fluid`, after QS, HQ, DQ and a PH under `Gas`, PT at (30 MPa, 700
/// K) and (1 bar, 300 K), PH at (30 MPa, h(700 K)) and DT at (10 mol/m³, 700 K) each return the state a fresh
/// `Fluid` returns, bit for bit, and PH is at 700 K.
#[cfg(feature = "fluids-all")]
#[test]
fn phase_hints_never_leak() {
    let truth = embedded("Water").state(pt(30e6, 700.0)).unwrap();
    let ph = Input::hp(Enthalpy::molar(truth.h(Basis::Molar)).unwrap(), Pressure::new(30e6).unwrap());
    let probes = [pt(30e6, 700.0), pt(1e5, 300.0), ph, dt(10.0, 700.0)];
    let fresh: Vec<_> = probes.iter().map(|&input| embedded("Water").state(input)).collect();
    let water = embedded("Water");
    let saturated = water.state(q_t(1.0, 400.0)).unwrap();
    for pair in [Pair::QS, Pair::HQ, Pair::DQ] {
        let prior = water.state(input_of(pair, &saturated).unwrap());
        assert!(matches!(prior, Ok(_) | Err(Error::Ambiguous { .. })), "{pair:?}: {prior:?}");
    }
    assert!(water.flash(ph, &FlashOptions::new().with_phase(Phase::Gas)).is_ok_and(|s| s.phase() == Phase::Gas));
    for (input, want) in probes.iter().zip(&fresh) {
        let got = water.state(*input);
        let bits = |r: &Result<State, Error>| r.as_ref().map(|s| format!("{s:?}")).map_err(|e| format!("{e:?}"));
        assert_eq!(bits(&got), bits(want), "{input:?}");
    }
    let back = water.state(ph).unwrap();
    assert!((back.t() / 700.0 - 1.0).abs() < 1e-9 && back.phase() == Phase::Supercritical, "{back:?}");
    assert_eq!(water.state(dt(10.0, 700.0)).map(|s| s.phase()), Ok(Phase::SupercriticalGas));
    assert_eq!(water.state(pt(1e5, 300.0)).map(|s| s.phase()), Ok(Phase::Liquid));
}

/// ROT-070 (map 03 §6 row 2): CoolProp honours an imposed phase in PT, DT and T+X, ignores it in DP and clears it
/// in PH, DH and HS. Here `with_phase` means one thing for all 19 pairs (D6). Water's liquid, gas, supercritical,
/// supercritical liquid and supercritical gas, and its saturated liquid, Q 0.5 and saturated vapour at 400 K; each
/// pair whose inputs a state defines, under each of six labels:
/// - every state returned, or every root of an `Ambiguous` (each taken by `Nearest`), carries the label and
///   reproduces the inputs;
/// - under the state's own label the state is returned (or is among the roots), but PT of a two-phase state, whose
///   (p, T) fixes no quality (`Undefined`);
/// - under `TwoPhase` a single-phase state's inputs are refused, their quality outside [0, 1] (`InvalidInput`) or
///   none, at p or T above the critical point's or for PT (`Undefined`), unless they have a two-phase root too (T+X
///   of a compressed liquid, M7.5), which is returned;
/// - a Q-pair under a single-phase label is refused: a quality is `Undefined` in that phase;
/// - no other refusal is `Unsupported`.
#[cfg(feature = "fluids-all")]
#[test]
fn phase_hint_is_honoured_by_every_pair() {
    let water = embedded("Water");
    let truths = [
        (pt(1e6, 350.0), Phase::Liquid),
        (pt(1e4, 400.0), Phase::Gas),
        (pt(30e6, 700.0), Phase::Supercritical),
        (pt(30e6, 400.0), Phase::SupercriticalLiquid),
        (pt(1e6, 700.0), Phase::SupercriticalGas),
        (q_t(0.0, 400.0), Phase::TwoPhase),
        (q_t(0.5, 400.0), Phase::TwoPhase),
        (q_t(1.0, 400.0), Phase::TwoPhase),
    ];
    let labels = [
        Phase::Liquid,
        Phase::Gas,
        Phase::TwoPhase,
        Phase::Supercritical,
        Phase::SupercriticalLiquid,
        Phase::SupercriticalGas,
    ];
    let mut checked = 0;
    for (input, phase) in truths {
        let truth = water.state(input).unwrap();
        assert_eq!(truth.phase(), phase, "{input:?}");
        checked +=
            product(Pair::ALL, &labels).map(|(pair, hint)| honours_the_hint(&water, &truth, pair, hint)).sum::<usize>();
    }
    assert_eq!(checked, 6 * (5 * 14 + 3 * 19));
}

/// One cell of [`phase_hint_is_honoured_by_every_pair`]: `pair` read off `truth` under the label `hint`, as that test
/// says; asserts it and returns 1, or 0 where the state defines no input of `pair`.
#[cfg(feature = "fluids-all")]
fn honours_the_hint(water: &Fluid, truth: &State, pair: Pair, hint: Phase) -> usize {
    let Some(input) = input_of(pair, truth) else { return 0 };
    let phase = truth.phase();
    let with = FlashOptions::new().with_phase(hint);
    let result = water.flash(input, &with);
    let states = states_of(water, input, with, &result);
    let at = format!("{hint:?} {input:?} of {truth:?}: {result:?}");
    assert!(states.iter().all(|s| s.phase() == hint && reproduces(s, input)), "{at}");
    let quality_pair = pair.vars().0 == Var::Q || pair.vars().1 == Var::Q;
    let no_quality = |phase| Err(Error::Undefined { prop: Prop::Q, phase });
    if quality_pair && hint != Phase::TwoPhase {
        assert_eq!(result, no_quality(hint), "{at}");
    } else if hint == phase && !(pair == Pair::PT && phase == Phase::TwoPhase) {
        assert!(states.iter().any(|s| is_truth(s, truth)), "{at}");
    } else if hint == Phase::TwoPhase && phase != Phase::TwoPhase {
        let refused = matches!(result, Err(Error::InvalidInput { quantity: Quantity::QUALITY, .. }))
            || result == no_quality(Phase::TwoPhase);
        assert!(refused || !states.is_empty(), "{at}");
    } else {
        assert!(!matches!(result, Err(Error::Unsupported { .. })), "{at}");
    }
    1
}

/// The fluids of the domain's tests: Water, CO₂ and Air.
#[cfg(feature = "fluids-all")]
const DOMAIN_FLUIDS: [&str; 3] = ["Water", "CarbonDioxide", "Air"];

/// `name`, its model's (Tmax, pmax), and its states beyond them under `Extrapolate`, each with whether it lies above
/// Tmax: at 1.25·Tmax and 10 MPa (true), and at 1000 K and 2·pmax (false).
#[cfg(feature = "fluids-all")]
fn beyond_the_domain(name: &str) -> (Fluid, (f64, f64), [(State, bool); 2]) {
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let fluid = embedded(name);
    let pure = record_of(name).builder().unwrap().build();
    let (t_max, p_max) = (pure.limits().t_max(), pure.limits().p_max());
    let hot = fluid.flash(pt(1e7, 1.25 * t_max), &extrapolate).unwrap();
    let dense = fluid.flash(pt(2.0 * p_max, 1000.0), &extrapolate).unwrap();
    (fluid, (t_max, p_max), [(hot, true), (dense, false)])
}

/// ROT-079 (map 03 §6 row 12): CoolProp's PT accepts 1e5 K for Water (Tmax 2000 K), its DP refuses T above
/// 1.5·Tmax, and its PT at 5 GPa fails inside the melting line's evaluator. Here one domain check applies to every
/// pair (D6, user decision 6). Water's, CO₂'s and Air's states at 1.25·Tmax and 10 MPa, and at 1000 K and 2·pmax:
/// each pair without a quality refuses them under `Enforce` with `AboveMaxTemperature` at the state's T, or
/// `AboveMaxPressure` at its p, or returns another root inside the domain (CO₂'s TU at 1000 K, at 244 MPa), and
/// under `Extrapolate` returns the state, flagged (or roots that include it). HS and SU search p up to pmax only, so
/// above it they refuse at pmax under either policy, as P+X refuses at 1.5·Tmax. States at Tmax and at pmax
/// themselves are in the domain ([`states_on_the_domain_bounds_are_in_it_for_every_pair`]); QT and PQ beyond them
/// name the saturation curve's end ([`qt_and_pq_beyond_the_domain_name_the_curve_end`]).
#[cfg(feature = "fluids-all")]
#[test]
fn domain_errors_are_uniform_across_pairs() {
    let mut checked = 0;
    for name in DOMAIN_FLUIDS {
        let (fluid, bounds, beyond) = beyond_the_domain(name);
        let cells = product(&beyond, Pair::ALL);
        checked += cells.map(|(truth, pair)| refused_beyond(name, &fluid, bounds, truth, pair)).sum::<usize>();
    }
    assert_eq!(checked, 3 * 2 * 14);
}

/// One cell of [`domain_errors_are_uniform_across_pairs`]: `pair` read off `truth`, beyond the domain's `bounds` (above
/// Tmax if `above_t`), refused under `Enforce` and flagged under `Extrapolate`, as that test says; asserts it and
/// returns 1, or 0 where the state defines no input of `pair`.
#[cfg(feature = "fluids-all")]
fn refused_beyond(name: &str, fluid: &Fluid, bounds: (f64, f64), (truth, above_t): (State, bool), pair: Pair) -> usize {
    let Some(input) = input_of(pair, &truth) else { return 0 };
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let at = format!("{name} {pair:?} of {truth:?}");
    let searched = !above_t && matches!(pair, Pair::HS | Pair::SU);
    let enforced = fluid.state(input);
    assert!(enforced_refuses(fluid, bounds, (&truth, above_t), searched, (input, &enforced)), "{at}: {enforced:?}");
    let flagged = fluid.flash(input, &extrapolate);
    if searched {
        assert_eq!(flagged, enforced, "{at}");
    } else {
        let states = states_of(fluid, input, extrapolate, &flagged);
        let ok = states.iter().any(|s| is_truth(s, &truth) && s.is_extrapolated());
        assert!(ok, "{at}: {flagged:?}");
    }
    1
}

/// Whether `Enforce` refused `input` (its result `enforced`), read off `truth` beyond the domain's (Tmax, pmax), at the
/// bound it crosses: at the state's T above Tmax (`above_t`), at its p above pmax, or at pmax itself for a pair that
/// searches p up to it (`searched`); or returns roots inside the domain, unflagged, that reproduce the input.
#[cfg(feature = "fluids-all")]
fn enforced_refuses(
    fluid: &Fluid,
    (t_max, p_max): (f64, f64),
    (truth, above_t): (&State, bool),
    searched: bool,
    (input, enforced): (Input, &Result<State, Error>),
) -> bool {
    let near = |a: f64, b: f64| (a / b - 1.0).abs() <= 1e-9;
    match *enforced {
        Err(Error::Domain(DomainError::AboveMaxTemperature { t, t_max: bound })) => {
            above_t && near(t, truth.t()) && bound == t_max
        }
        Err(Error::Domain(DomainError::AboveMaxPressure { p, p_max: bound })) => {
            !above_t && bound == p_max && if searched { p == p_max } else { near(p, truth.p()) }
        }
        // A pair with several roots returns the one in the domain (CO₂'s TU at 1000 K: 244 MPa).
        _ => states_of(fluid, input, FlashOptions::new(), enforced)
            .iter()
            .all(|s| reproduces(s, input) && s.t() <= t_max && s.p() <= p_max && !s.is_extrapolated()),
    }
}

/// ROT-079: on the domain's bounds every state is in the domain. Water's, CO₂'s and Air's PT states at Tmax and 10
/// MPa and at pmax and 1000 K round-trip through every pair without a quality under `Enforce` (or are among its
/// roots).
#[cfg(feature = "fluids-all")]
#[test]
fn states_on_the_domain_bounds_are_in_it_for_every_pair() {
    let mut checked = 0;
    for name in DOMAIN_FLUIDS {
        let (fluid, (t_max, p_max), _) = beyond_the_domain(name);
        let truths = [pt(1e7, t_max), pt(p_max, 1000.0)].map(|input| fluid.state(input).unwrap());
        for (truth, pair) in product(&truths, Pair::ALL) {
            let Some(input) = input_of(pair, &truth) else { continue };
            let result = fluid.state(input);
            let states = states_of(&fluid, input, FlashOptions::new(), &result);
            assert!(states.iter().any(|s| is_truth(s, &truth)), "{name} {pair:?} of {truth:?}: {result:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 3 * 2 * 14);
}

/// ROT-079: QT at the T of Water's, CO₂'s and Air's states above Tmax, and PQ at the p of their states above pmax, lie
/// beyond the saturation curve, and under either policy are refused at the curve's end (below the domain's bound),
/// which the error names. QS, HQ and DQ have no input beyond the domain: every saturated state lies below the
/// critical point.
#[cfg(feature = "fluids-all")]
#[test]
fn qt_and_pq_beyond_the_domain_name_the_curve_end() {
    let policies = [FlashOptions::new(), FlashOptions::new().with_domain(DomainPolicy::Extrapolate)];
    let mut checked = 0;
    for name in DOMAIN_FLUIDS {
        let (fluid, bounds, beyond) = beyond_the_domain(name);
        for ((truth, above_t), policy) in product(&beyond, &policies) {
            let qt = Input::qt(Quality::new(1.0).unwrap(), Temperature::new(truth.t()).unwrap());
            let pq = Input::pq(Pressure::new(truth.p()).unwrap(), Quality::new(1.0).unwrap());
            let beyond_curve = if above_t { qt } else { pq };
            let got = fluid.flash(beyond_curve, &policy);
            assert!(names_the_curve_end(&got, (&truth, above_t), bounds), "{name} {beyond_curve:?}: {got:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 3 * 2 * 2);
}

/// Whether `got` refuses a Q-pair at `truth`'s T (`above_t`) or p beyond the curve's end, which it names below the
/// domain's (Tmax, pmax).
#[cfg(feature = "fluids-all")]
fn names_the_curve_end(
    got: &Result<State, Error>,
    (truth, above_t): (&State, bool),
    (t_max, p_max): (f64, f64),
) -> bool {
    match *got {
        Err(Error::Domain(DomainError::AboveMaxTemperature { t, t_max: end })) => {
            above_t && t == truth.t() && end < t_max
        }
        Err(Error::Domain(DomainError::AboveMaxPressure { p, p_max: end })) => {
            !above_t && p == truth.p() && end < p_max
        }
        _ => false,
    }
}

/// ROT-007 (map 01 R4; map 12 COO-6): CoolProp's DmassT returns a density off its input, its HmassT fails where
/// HmolarT works, and it has no SU arm. Here the basis is converted once, at the edge (`Fluid::native`): DT in
/// kg/m³ returns the input density to two ulp in that basis, and every pair read off a state in mass units returns
/// the state its molar inputs return, or the same roots, within class `Flash` (T and ρ within 1e-9, Q within 1e-9:
/// by CO₂'s critical point, an ulp of u moves PU's ρ by 7e-12). Water's liquid, gas, supercritical state and Q 0.5
/// at 400 K; CO₂'s near its critical point.
#[cfg(feature = "fluids-all")]
#[test]
fn dt_mass_basis_reproduces_input() {
    let mut checked = 0;
    for (name, inputs) in [
        ("Water", [pt(1e6, 350.0), pt(1e4, 400.0), pt(30e6, 700.0), q_t(0.5, 400.0)]),
        ("CarbonDioxide", [pt(7.4e6, 305.0), pt(7.3e6, 300.0), dt(10_000.0, 304.2), q_t(0.5, 304.0)]),
    ] {
        let fluid = embedded(name);
        for truth in inputs.map(|input| fluid.state(input).unwrap()) {
            let mass = truth.rho(Basis::Mass);
            let dt_mass = Input::dt(Density::mass(mass).unwrap(), Temperature::new(truth.t()).unwrap());
            let back = fluid.state(dt_mass).unwrap().rho(Basis::Mass);
            assert!((back - mass).abs() <= 2.0 * f64::EPSILON * mass, "{name}: {back} against {mass}");
            checked += Pair::ALL.iter().map(|&pair| same_in_mass_units(name, &fluid, &truth, pair)).sum::<usize>();
        }
    }
    assert!(checked >= 4 * 14 + 2 * 19, "{checked}");
}

/// One cell of [`dt_mass_basis_reproduces_input`]: `pair` read off `truth` in mass units returns the states (or the
/// roots) its molar inputs return, each `truth`'s within class `Flash` ([`is_truth`]); asserts it and returns 1, or 0
/// where the state defines no input of `pair`.
#[cfg(feature = "fluids-all")]
fn same_in_mass_units(name: &str, fluid: &Fluid, truth: &State, pair: Pair) -> usize {
    let Some(molar) = input_of(pair, truth) else { return 0 };
    let m = fluid.info().molar_mass();
    let (vx, vy) = pair.vars();
    let (x, y) = molar.values();
    let per_kg = |var: Var, v: f64| match var {
        Var::D => v * m,
        Var::H | Var::S | Var::U => v / m,
        _ => v,
    };
    let mass_input = Input::new(pair, per_kg(vx, x), per_kg(vy, y), Basis::Mass).unwrap();
    let opts = FlashOptions::new();
    let (a, b) = (fluid.state(molar), fluid.state(mass_input));
    let (sa, sb) = (states_of(fluid, molar, opts, &a), states_of(fluid, mass_input, opts, &b));
    let same = !sa.is_empty() && sa.len() == sb.len() && sa.iter().zip(&sb).all(|(s, t)| is_truth(s, t));
    assert!(same, "{name} {pair:?}: {a:?} against {b:?}");
    1
}

/// PLAN.md M8.11 (D6, user decision 6): one melting-line check applies to every pair, after it solves. Water's ice VI at
/// 1 GPa and 290 K, CO₂'s solid at 100 MPa and 230 K (#1936) and Nitrogen's at 500 MPa and 70 K: each pair without a
/// quality refuses them under `Enforce` with `BelowMeltingLine` at the state's T and T_melt(p), or returns another root
/// above the line, and under `Extrapolate` returns the state, flagged (or roots that include it). CO₂ 1 mK above its
/// line at 7.38 MPa is in the domain (#2608: CoolProp allows 1 mK below it).
#[cfg(feature = "fluids-all")]
#[test]
fn enforce_refuses_below_the_melting_line() {
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let mut checked = 0;
    for (name, p, t) in [("Water", 1e9, 290.0), ("CarbonDioxide", 100e6, 230.0), ("Nitrogen", 500e6, 70.0)] {
        let fluid = embedded(name);
        let truth = fluid.flash(pt(p, t), &extrapolate).unwrap();
        assert!(truth.is_extrapolated(), "{name}");
        let t_melt = fluid.model().melting_temperature(p).unwrap();
        for pair in Pair::ALL {
            let Some(input) = input_of(*pair, &truth) else { continue };
            let at = format!("{name} {pair:?} of {truth:?}");
            let flagged = fluid.flash(input, &extrapolate);
            let states = states_of(&fluid, input, extrapolate, &flagged);
            assert!(states.iter().any(|s| is_truth(s, &truth) && s.is_extrapolated()), "{at}: {flagged:?}");
            assert!(below_the_line_refused(&fluid, input, (&truth, t_melt)), "{at}");
            checked += 1;
        }
    }
    assert_eq!(checked, 3 * 14);
    let co2 = embedded("CarbonDioxide");
    let t_melt = co2.model().melting_temperature(7.38e6).unwrap();
    assert!((t_melt - 218.049).abs() < 1e-3, "{t_melt}");
    assert!(co2.state(pt(7.38e6, t_melt + 1e-3)).is_ok());
    // At the state's own p, its EOS's at (T, ρ), 1e-13 K from T_melt(7.38 MPa).
    let below = co2.state(pt(7.38e6, t_melt - 1e-3));
    let refused = |t: f64, at: f64| t == t_melt - 1e-3 && (at - t_melt).abs() < 1e-9;
    assert!(matches!(below, Err(Error::Domain(DomainError::BelowMeltingLine { t, t_melt: at })) if refused(t, at)));
}

/// Whether `Enforce` refuses `input`, read off `truth` below the melting line where T_melt is `t_melt`: each of its
/// results (every root of an `Ambiguous`, each taken by `Nearest`) is a `BelowMeltingLine` (Nitrogen's TU at 70 K has
/// two solid roots) or a state above the line that reproduces the input, and one is the truth's refusal, at its T and
/// T_melt.
#[cfg(feature = "fluids-all")]
fn below_the_line_refused(fluid: &Fluid, input: Input, (truth, t_melt): (&State, f64)) -> bool {
    let near = |a: f64, b: f64| (a / b - 1.0).abs() <= 1e-9;
    let results = match fluid.state(input) {
        Err(Error::Ambiguous { roots }) => roots
            .as_slice()
            .iter()
            .map(|&root| fluid.flash(input, &FlashOptions::new().with_roots(RootPolicy::Nearest(root))))
            .collect(),
        result => vec![result],
    };
    let refusal = |r: &Result<State, Error>| match *r {
        Err(Error::Domain(DomainError::BelowMeltingLine { t, t_melt: at })) => Some((t, at)).filter(|(t, at)| t < at),
        _ => None,
    };
    let above = |r: &Result<State, Error>| r.as_ref().is_ok_and(|s| reproduces(s, input) && !s.is_extrapolated());
    let truths = |r: &Result<State, Error>| refusal(r).is_some_and(|(t, at)| near(t, truth.t()) && near(at, t_melt));
    results.iter().all(|r| refusal(r).is_some() || above(r)) && results.iter().any(truths)
}
