//! P+X along the isobar (PLAN.md M7.3).

use super::*;
#[cfg(feature = "fluids-all")]
use crate::flash::cubic::Cubic;
#[cfg(feature = "fluids-all")]
use crate::flash::isobar::{Span, cubic_seed, pt_state};

/// The P+X inputs of `state`, molar: (p, h), (p, s) and (p, u).
#[cfg(feature = "fluids-all")]
fn p_x(state: &State) -> [Input; 3] {
    let (p, m) = (Pressure::new(state.p()).unwrap(), Basis::Molar);
    [
        Input::hp(Enthalpy::molar(state.h(m)).unwrap(), p),
        Input::ps(p, Entropy::molar(state.s(m)).unwrap()),
        Input::pu(p, InternalEnergy::molar(state.u(m)).unwrap()),
    ]
}

/// PH, PS and PU of the PT state of `fluid` at (p, T) return its phase, and its T and ρ within class `Flash`
/// (1e-9), solved by Newton in (T, ln ρ) in at most 12 iterations (issue #143); the inputs checked.
#[cfg(feature = "fluids-all")]
fn round_trips(fluid: &Fluid, p: f64, t: f64) -> usize {
    let truth = fluid.state(pt(p, t)).unwrap();
    let m = Basis::Molar;
    for input in p_x(&truth) {
        let back = fluid.state(input).unwrap_or_else(|e| panic!("{input:?} at {t} K: {e:?}"));
        let near = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-9;
        assert!(
            near(back.t(), t) && near(back.rho(m), truth.rho(m)) && back.phase() == truth.phase(),
            "{input:?}: {back:?} against {truth:?}"
        );
        let path = back.path();
        assert!(path.strategy == Strategy::TRhoNewton && path.iterations <= 12, "{input:?} at {t} K: {path:?}");
    }
    3
}

/// map 03 §8 PXFlash (`CoolProp-Tests-PXFlash.cpp:34`, PLAN.md M7.3): Nitrogen in the supercritical-cold corner,
/// above its critical pressure and below its critical temperature, a supercritical liquid. CoolProp's master threw
/// on PS here until its #3020; 8.0.0 solves both, T within 2.5e-12 (PH 1.2e-11). At 86.35 K and 4.754 MPa and at
/// 90.20 K and 7.768 MPa, PS, PH and PU return the PT state within class `Flash` (CoolProp's own test asks 1e-5).
#[cfg(feature = "fluids-all")]
#[test]
fn nitrogen_supercritical_cold_ps() {
    let nitrogen = embedded("Nitrogen");
    for (t, p) in [(86.35, 4.754e6), (90.20, 7.768e6)] {
        assert_eq!(nitrogen.state(pt(p, t)).unwrap().phase(), Phase::SupercriticalLiquid);
        assert_eq!(round_trips(&nitrogen, p, t), 3);
    }
}

/// map 03 §8 PXcdj (`CoolProp-Tests-PXcdj.cpp:186`, PLAN.md M7.3): CoolProp's named P+X regressions, gas, liquid
/// and supercritical states of six fluids and the hard ones (Nitrogen's supercritical-cold trap at 121 K and 3.7
/// MPa, Water by its density maximum at 277 K and 5 MPa; its R134a "spinodal step" at 250 K and 0.3 MPa is its
/// R134a liquid, listed once here). CoolProp asks T and ρ within 1e-5 of the PT state; here PH, PS and PU return
/// them within class `Flash`.
#[cfg(feature = "fluids-all")]
#[test]
fn pxcdj_named_regressions() {
    let cases = [
        ("Water", 600.0, 1.0e5),
        ("Water", 320.0, 5.0e6),
        ("Water", 700.0, 30.0e6),
        ("CarbonDioxide", 350.0, 1.0e6),
        ("CarbonDioxide", 320.0, 10.0e6),
        ("CarbonDioxide", 250.0, 5.0e6),
        ("R134a", 250.0, 3.0e5),
        ("R134a", 350.0, 2.0e5),
        ("n-Propane", 350.0, 1.0e6),
        ("n-Propane", 250.0, 5.0e6),
        ("Nitrogen", 300.0, 1.0e5),
        ("Nitrogen", 200.0, 10.0e6),
        ("MM", 450.0, 1.0e5),
        ("MM", 350.0, 5.0e6),
        ("Nitrogen", 121.0, 3.7e6),
        ("Water", 277.0, 5.0e6),
    ];
    let checked: usize = cases.iter().map(|&(name, t, p)| round_trips(&embedded(name), p, t)).sum();
    assert_eq!(checked, 3 * cases.len());
}

/// Issue #143: where Peng and Robinson's seed lies inside the model's spinodal, Newton in (T, ln ρ) stops at its first
/// iterate and the search in T answers: R32's supercritical liquid at 5.95 MPa and 351.0 K, just above its critical
/// point (5.782 MPa, 351.255 K), where the cubic's density is the model's unstable one.
#[cfg(feature = "fluids-all")]
#[test]
fn p_x_searches_in_t_where_the_cubic_seeds_an_unstable_state() {
    let r32 = embedded("R32");
    let truth = r32.state(pt(5_954_428.977_780_855, 351.001_875)).unwrap();
    for input in p_x(&truth) {
        let back = r32.state(input).unwrap();
        assert!((back.t() / truth.t() - 1.0).abs() < 1e-9 && back.path().strategy == Strategy::Isobar, "{back:?}");
    }
}

/// Issue #143: Newton in (T, ln ρ) keeps a state only if it is PT's at its (p, T) and reproduces p and x: Water's
/// compressed liquid at 5 MPa and 320 K is PT's liquid for its own h, and refused for an h 1 J/mol away.
#[cfg(feature = "fluids-all")]
#[test]
fn p_x_newton_keeps_only_a_state_that_reproduces_x() {
    let water = record_of("Water").compile().unwrap();
    let (p, t) = (5e6, 320.0);
    let rho = embedded("Water").state(pt(p, t)).unwrap().rho(Basis::Molar);
    let b = total(&water, t, rho).unwrap();
    let h = Caloric::H.at(water.eos().gas_constant(), t, &b);
    let span = Span { lo: 273.16, hi: 537.0, ends: (None, None), liquid: true, edge: None };
    let opts = FlashOptions::new();
    let branch = pt_state(&water, (p, h, Caloric::H), &opts, &span, (t, rho, &b)).map(|branch| branch.phase);
    assert_eq!(branch, Some(Phase::Liquid));
    assert!(pt_state(&water, (p, h + 1.0, Caloric::H), &opts, &span, (t, rho, &b)).is_none());
}

/// Issue #143: the seed on a span without a saturated end is Peng and Robinson's T at (p, x), where the model's ideal
/// gas plus the cubic's departure is x (to 1e-9 of RT), and its density there: Methane at 10 MPa, above its critical
/// pressure, for the h, s and u of its state at 250 K, which the seed's T lies within 3 % of.
#[cfg(feature = "fluids-all")]
#[test]
fn the_cubic_seed_solves_the_cubics_x() {
    let methane = record_of("Methane").compile().unwrap();
    let (p, t) = (10e6, 250.0);
    let truth = embedded("Methane").state(pt(p, t)).unwrap();
    let (t_low, t_high) = t_search(&methane);
    let span = Span { lo: t_low, hi: t_high, ends: (None, None), liquid: false, edge: None };
    let (eos, crit) = (methane.eos(), methane.critical_point().unwrap());
    let r = eos.gas_constant();
    let cubic = Cubic::new(crit.t, crit.p, methane.info().constants().acentric.unwrap(), r);
    let m = Basis::Molar;
    for (var, x) in [(Caloric::H, truth.h(m)), (Caloric::S, truth.s(m)), (Caloric::U, truth.u(m))] {
        let (t_seed, rho) = cubic_seed(&methane, p, x, var, &span).unwrap();
        let ideal = eos.ideal(t_seed, p / (r * t_seed), crate::derivs::Order::Two).bundle().unwrap();
        let cubic_x = var.at(r, t_seed, &ideal) + cubic.departure(var, t_seed, p, rho).unwrap();
        let scale = if var == Caloric::S { r } else { r * t_seed };
        assert!((cubic_x - x).abs() < 1e-9 * scale && rho == cubic.density(t_seed, p).unwrap(), "{var:?}: {t_seed}");
        assert!((t_seed / t - 1.0).abs() < 0.03, "{var:?}: {t_seed} K");
    }
}

/// Issue #143: under an imposed phase P+X's Newton in (T, ln ρ) does not run (its branch check is PT's unimposed
/// rule), and the search in T answers: Water's compressed liquid at 5 MPa and 320 K, under `Liquid`.
#[cfg(feature = "fluids-all")]
#[test]
fn p_x_under_a_hint_searches_in_t() {
    let water = embedded("Water");
    let truth = water.state(pt(5e6, 320.0)).unwrap();
    for input in p_x(&truth) {
        let back = water.flash(input, &FlashOptions::new().with_phase(Phase::Liquid)).unwrap();
        assert!((back.t() / 320.0 - 1.0).abs() < 1e-9 && back.path().strategy == Strategy::Isobar, "{back:?}");
    }
}

/// The P+X acceptance gate (D6, `reproduces_x`): the solved state's x within 1e-9 of its scale, |x| or at least RT
/// for h and u, R for s; at the bound exactly, reproduced.
#[test]
fn x_is_reproduced_to_its_scale() {
    let (r, t) = (8.0, 300.0);
    assert_eq!([Caloric::H, Caloric::S, Caloric::U].map(|v| v.scale(r, t, 10.0)), [2400.0, 10.0, 2400.0]);
    assert_eq!([Caloric::H, Caloric::S, Caloric::U].map(|v| v.scale(r, t, -5e3)), [5e3; 3]);
    let bound = REPRODUCED_X * 2400.0;
    assert!(reproduces_x(Caloric::H, r, t, 0.0, bound) && !reproduces_x(Caloric::H, r, t, 0.0, 2.0 * bound));
    assert!(reproduces_x(Caloric::H, r, t, 1000.0, 1000.0 + 1e-6));
    assert!(reproduces_x(Caloric::S, r, t, 0.0, 8e-9) && !reproduces_x(Caloric::S, r, t, 0.0, 1e-8));
}

/// P+X at the dome's edge and under an imposed phase (PLAN.md M7.3, D6). Water at 1 MPa: PH, PS and PU of PQ's
/// saturated liquid and vapour are two-phase at Q = 0 and 1 exactly, the lever rule's ends. h a little above the
/// saturated liquid's is two-phase, Q 0.001; under `Liquid` it is the superheated liquid, above the saturation
/// temperature; under `TwoPhase` the same two-phase state; and the liquid's own h under `TwoPhase` is refused (a
/// quality below 0). Under `TwoPhase` (M7.9) an x past a saturated side's by 1e-11 of it, within the acceptance
/// gate, is that side, Q 0 or 1 exactly, and one past it by 1e-7 is refused.
#[cfg(feature = "fluids-all")]
#[test]
fn p_x_at_the_dome_edge_and_under_a_hint() {
    let water = embedded("Water");
    let m = Basis::Molar;
    for q in [0.0, 1.0] {
        let saturated = water.state(p_q(1e6, q)).unwrap();
        for input in p_x(&saturated) {
            let back = water.state(input).unwrap();
            assert!(back.phase() == Phase::TwoPhase && back.quality() == Some(q), "{input:?}: {back:?}");
        }
    }
    let (liquid, vapour) = (water.state(p_q(1e6, 0.0)).unwrap(), water.state(p_q(1e6, 1.0)).unwrap());
    let h = liquid.h(m) + 1e-3 * (vapour.h(m) - liquid.h(m));
    let ph = Input::hp(Enthalpy::molar(h).unwrap(), Pressure::new(1e6).unwrap());
    let free = water.state(ph).unwrap();
    assert!(free.phase() == Phase::TwoPhase && free.quality().is_some_and(|q| (q - 1e-3).abs() < 1e-12), "{free:?}");
    let with = |phase| water.flash(ph, &FlashOptions::new().with_phase(phase));
    let superheated = with(Phase::Liquid).unwrap();
    assert!(superheated.phase() == Phase::Liquid && superheated.t() > liquid.t(), "{superheated:?}");
    assert!((superheated.h(m) / h - 1.0).abs() < 1e-12, "{superheated:?}");
    assert_eq!(with(Phase::TwoPhase), Ok(free));
    let compressed = water.state(pt(1e6, 400.0)).unwrap();
    let refused = water.flash(p_x(&compressed)[0], &FlashOptions::new().with_phase(Phase::TwoPhase));
    assert!(matches!(refused, Err(Error::InvalidInput { quantity: "quality", .. })), "{refused:?}");
    let (p, two_phase) = (Pressure::new(1e6).unwrap(), FlashOptions::new().with_phase(Phase::TwoPhase));
    for ((side, q, outwards), share) in product(&[(liquid, 0.0, -1.0), (vapour, 1.0, 1.0)], &[1e-11, 1e-7]) {
        let past = |x: f64| x * (1.0 + outwards * share);
        let inputs = [
            Input::hp(Enthalpy::molar(past(side.h(m))).unwrap(), p),
            Input::ps(p, Entropy::molar(past(side.s(m))).unwrap()),
            Input::pu(p, InternalEnergy::molar(past(side.u(m))).unwrap()),
        ];
        for input in inputs {
            let back = water.flash(input, &two_phase);
            assert!(past_a_side_is_gated(&back, q, share), "{input:?}: {back:?}");
        }
    }
}

/// Whether a P+X under `TwoPhase` of an x past the side at quality `q` by `share` of it is as the acceptance gate
/// says: within it (1e-11) that side, Q `q` exactly; beyond it (1e-7) refused, its quality outside [0, 1].
#[cfg(feature = "fluids-all")]
fn past_a_side_is_gated(back: &Result<State, Error>, q: f64, share: f64) -> bool {
    match back {
        Ok(state) => share < 1e-9 && state.phase() == Phase::TwoPhase && state.quality() == Some(q),
        Err(e) => share > 1e-9 && matches!(e, Error::InvalidInput { quantity: "quality", .. }),
    }
}

/// P+X on a stale curve (M6.6): the curve only seeds the VLE at p, which decides. Nitrogen with its EOS edited away
/// from its superancillary: PH, PS and PU of its two-phase DT state at 100 K are that state, its T the VLE's within
/// 1e-9, where the stale curve's saturation temperature at that p is 2e-7 off.
#[cfg(feature = "fluids-all")]
#[test]
fn p_x_on_a_stale_curve_follows_the_vle() {
    let mut record = record_of("Nitrogen");
    record.eos.power.first_mut().unwrap().n *= 1.0 + 1e-6;
    let stale = record.superancillary_curve().unwrap();
    let curve_of = record.clone();
    let pure = record.builder().unwrap().lazy_saturation(move || Ok(curve_of.superancillary_curve())).build();
    let fluid = Fluid::new(Arc::new(pure));
    let truth = fluid.state(dt(5_000.0, 100.0)).unwrap();
    assert_eq!((truth.phase(), truth.path().strategy), (Phase::TwoPhase, Strategy::Vle));
    assert!((stale.at_p(truth.p()).unwrap().bubble.t / 100.0 - 1.0).abs() > 1e-8, "the edit moved saturation");
    for input in p_x(&truth) {
        let back = fluid.state(input).unwrap();
        let q = (back.quality().unwrap() - truth.quality().unwrap()).abs();
        assert!((back.t() / 100.0 - 1.0).abs() < 1e-9 && q < 1e-9, "{input:?}: {back:?}");
    }
}

/// ROT-021 (map 03 §6 rows 17, 24): CoolProp's P+X errors are untyped strings, and its PH and PS brackets can raise
/// "Pressure to PQ_flash …", naming another pair. Here every P+X error is typed, with its numbers or its own pair:
/// Water's PH above pmax is `AboveMaxPressure`; an h whose state lies between Tmax and the search's 1.5·Tmax is
/// `AboveMaxTemperature` at its T, and flagged under `Extrapolate`; an h hotter than 1.5·Tmax is
/// `AboveMaxTemperature` at that bound; an s colder than the liquid at the triple point is `BelowMinTemperature` at
/// the triple point. A curve without a pressure inverse leaves PH `Unsupported { pair: HP }`, not PQ, and a model
/// with no saturation and no critical point leaves PU `Unsupported { pair: PU }`, not PT. A solved state's path
/// names its strategy.
#[cfg(feature = "fluids-all")]
#[test]
fn errors_name_the_failing_strategy() {
    let water = embedded("Water");
    let (m, extrapolate) = (Basis::Molar, FlashOptions::new().with_domain(DomainPolicy::Extrapolate));
    let hot = |t: f64| water.flash(pt(1e6, t), &extrapolate).unwrap().h(m);
    let ph = |h: f64, p: f64| Input::hp(Enthalpy::molar(h).unwrap(), Pressure::new(p).unwrap());
    let above_pmax = DomainError::AboveMaxPressure { p: 2e9, p_max: 1e9 };
    assert_eq!(water.state(ph(hot(1000.0), 2e9)), Err(above_pmax.into()));
    let Err(Error::Domain(DomainError::AboveMaxTemperature { t, t_max })) = water.state(ph(hot(2500.0), 1e6)) else {
        panic!("{:?}", water.state(ph(hot(2500.0), 1e6)))
    };
    assert!((t / 2500.0 - 1.0).abs() < 1e-9 && t_max == 2000.0, "{t}");
    let flagged = water.flash(ph(hot(2500.0), 1e6), &extrapolate).unwrap();
    assert!(flagged.is_extrapolated() && (flagged.t() / 2500.0 - 1.0).abs() < 1e-9, "{flagged:?}");
    let beyond = DomainError::AboveMaxTemperature { t: 3000.0, t_max: 2000.0 };
    assert_eq!(water.flash(ph(hot(3500.0), 1e6), &extrapolate), Err(beyond.into()));
    let cold = water.state(pt(1e6, 273.16)).unwrap().s(m) - 1.0;
    let ps = Input::ps(Pressure::new(1e6).unwrap(), Entropy::molar(cold).unwrap());
    assert_eq!(water.state(ps), Err(DomainError::BelowMinTemperature { t: 273.16, t_min: 273.16 }.into()));

    let record = FluidRecord::synthetic("X").unwrap();
    let fit = Fluid::new(Arc::new(record.builder().unwrap().saturation(Fit).build()));
    assert_eq!(fit.state(ph(0.0, 1e6)), Err(Error::Unsupported { pair: Pair::HP }));
    let bare = Fluid::new(Arc::new(FluidRecord::synthetic("Y").unwrap().builder().unwrap().build()));
    let pu = Input::pu(Pressure::new(1e6).unwrap(), InternalEnergy::molar(0.0).unwrap());
    assert_eq!(bare.state(pu), Err(Error::Unsupported { pair: Pair::PU }));

    let solved = water.state(ph(hot(500.0), 1e6)).unwrap();
    assert!(solved.path().strategy == Strategy::TRhoNewton && solved.path().iterations > 0, "{:?}", solved.path());
}
