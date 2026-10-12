//! PT (PLAN.md M7.1, M7.2): the branch rule, the bracketed density solve, a guess as a seed and the two roots at
//! the saturation pressure.

use super::*;

/// ROT-074 (map 03 §6): PT's state is built at the density its solve returned, so its own p is the input to the
/// rounding of p's terms; CoolProp's PT of Nitrogen at 10 MPa and 200 K reports p 3.3e-9 off.
#[cfg(feature = "fluids-all")]
#[test]
fn pt_reproduces_its_input_pressure() {
    let state = embedded("Nitrogen").state(pt(10e6, 200.0)).unwrap();
    assert_eq!((state.phase(), state.path().strategy), (Phase::Supercritical, Strategy::DensityNewton));
    assert!((state.p() / 10e6 - 1.0).abs() < 1e-15, "{}", state.p());
}

/// PT's branch rule at its edges (PLAN.md M7.1), exactly: Water at its labelling critical point (the
/// superancillary's top) is that point, ρc with no solve; at Tc above pc supercritical; at pc above Tc a
/// supercritical gas (above pc supercritical), and at pc below Tc a liquid (above pc a supercritical liquid).
#[cfg(feature = "fluids-all")]
#[test]
fn pt_branch_edges_are_exact() {
    let water = embedded("Water");
    let curve = record_of("Water").superancillary_curve().unwrap();
    let t_c = curve.t_range().1;
    let top = curve.at_t(t_c).unwrap();
    let (p_c, rho_c) = (top.bubble.p, top.bubble.rho);
    let critical = water.state(pt(p_c, t_c)).unwrap();
    assert_eq!((critical.phase(), critical.rho(Basis::Molar), critical.path()), (Phase::CriticalPoint, rho_c, DIRECT));
    let phase = |p, t| water.state(pt(p, t)).map(|s| s.phase());
    assert_eq!(
        [phase(2.0 * p_c, t_c), phase(p_c, 1.1 * t_c), phase(1.5 * p_c, 1.1 * t_c)],
        [Ok(Phase::Supercritical), Ok(Phase::SupercriticalGas), Ok(Phase::Supercritical)]
    );
    assert_eq!(
        [phase(p_c, 0.9 * t_c), phase(1.5 * p_c, 0.9 * t_c)],
        [Ok(Phase::Liquid), Ok(Phase::SupercriticalLiquid)]
    );
}

/// PT checks its domain (D6): Water at 3000 K (its Tmax 2000 K) is refused under `Enforce` and flagged under
/// `Extrapolate`; inside the domain it is not flagged.
#[cfg(feature = "fluids-all")]
#[test]
fn pt_outside_the_domain_is_refused_or_flagged() {
    let water = embedded("Water");
    let hot = DomainError::AboveMaxTemperature { t: 3000.0, t_max: 2000.0 };
    assert_eq!(water.state(pt(1e6, 3000.0)), Err(hot.into()));
    let flagged = water.flash(pt(1e6, 3000.0), &FlashOptions::new().with_domain(DomainPolicy::Extrapolate)).unwrap();
    assert!(flagged.is_extrapolated() && flagged.phase() == Phase::SupercriticalGas, "{flagged:?}");
    assert!(!water.state(pt(1e6, 1000.0)).unwrap().is_extrapolated());
}

/// An imposed phase is its own branch (PLAN.md M7.1): Water at 1 bar and 400 K is a vapour by the rule (psat
/// 245.8 kPa); under `Liquid` it is the metastable liquid, seeded by ρ′ and found in a few steps, under `Gas` the
/// vapour, and under `TwoPhase` refused, since (p, T) fix no quality.
#[cfg(feature = "fluids-all")]
#[test]
fn pt_under_a_hint_follows_its_branch() {
    let water = embedded("Water");
    let with = |phase| water.flash(pt(1e5, 400.0), &FlashOptions::new().with_phase(phase));
    let (liquid, gas) = (with(Phase::Liquid).unwrap(), with(Phase::Gas).unwrap());
    assert!(liquid.rho(Basis::Molar) > 50_000.0 && liquid.phase() == Phase::Liquid, "{liquid:?}");
    assert!(liquid.path().iterations <= 3, "from ρ′, not the far end: {:?}", liquid.path());
    assert!((gas.rho(Basis::Molar) - 30.3).abs() < 0.1 && gas.phase() == Phase::Gas, "{gas:?}");
    assert_eq!(with(Phase::TwoPhase), Err(Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase }));
    assert_eq!(water.state(pt(1e5, 400.0)).unwrap().phase(), Phase::Gas);
}

/// PLAN.md M7.1, the nightly grid: OrthoHydrogen at 1.71 GPa and 174.23 K is denser than its model's largest
/// density, ρ(T_min, p_max), since its EOS is not densest at T_min there: PT's bracket grows past it.
#[cfg(feature = "fluids-all")]
#[test]
fn pt_grows_its_bracket_past_rho_max() {
    let h2 = embedded("OrthoHydrogen");
    let (p, t) = (1_710_994_283.990_256_8, 174.2317);
    let state = h2.state(pt(p, t)).unwrap();
    let rho_max = h2.model().helmholtz().unwrap().rho_max(t);
    assert!(state.rho(Basis::Molar) > rho_max, "{} against {rho_max}", state.rho(Basis::Molar));
    assert!((state.p() / p - 1.0).abs() < 1e-14, "{}", state.p());
}

/// A pseudo-pure fluid's PT follows its pressure ancillaries exactly, as CoolProp's: R410A at 280 K is liquid one
/// float above the bubble pressure and gas one float below the dew pressure; at either, or between them, (p, T) is
/// two-phase and fixes no quality.
#[cfg(feature = "fluids-all")]
#[test]
fn pseudo_pure_pt_follows_its_ancillaries() {
    let r410a = embedded("R410A");
    let definition = record_of("R410A").pseudo_pure.unwrap();
    let (p_l, p_v) = (definition.p_l.at(280.0), definition.p_v.at(280.0));
    let phase = |p: f64| r410a.state(pt(p, 280.0)).map(|s| s.phase());
    let (above, below) = (p_l.next_up(), p_v.next_down());
    assert_eq!([phase(above), phase(below)], [Ok(Phase::Liquid), Ok(Phase::Gas)]);
    let no_quality = Err(Error::Undefined { prop: Prop::Q, phase: Phase::TwoPhase });
    assert_eq!(
        [phase(p_l), phase(p_v), phase(0.5 * (p_l + p_v))],
        [no_quality.clone(), no_quality.clone(), no_quality]
    );
}

/// ROT-072 (map 03 §6): a guess only seeds PT's bracketed solve. CoolProp's PT of Water at 10 MPa and 300 K from a
/// guess of 0.9 ρ lands on dp/dρ = −2.95e5 < 0, ρ 14.2 % off; here 0.9 ρ, a vapour's density, twice ρ and a guess
/// made at another temperature all give the stable liquid root to rounding.
#[cfg(feature = "fluids-all")]
#[test]
fn a_guess_is_only_a_seed() {
    let water = embedded("Water");
    let rho = water.state(pt(10e6, 300.0)).unwrap().rho(Basis::Molar);
    let dp_drho = Partial { of: DerivVar::P, wrt: DerivVar::Dmolar, at: DerivVar::T };
    for (t, guess) in [(300.0, 0.9 * rho), (300.0, 4.0), (300.0, 2.0 * rho), (500.0, rho)] {
        let state = water.flash(pt(10e6, 300.0), &FlashOptions::new().with_guess(t, guess)).unwrap();
        assert!((state.rho(Basis::Molar) / rho - 1.0).abs() < 1e-14, "guess {guess}: {state:?}");
        assert!(state.partial(dp_drho).unwrap() > 0.0 && state.phase() == Phase::Liquid, "{state:?}");
    }
}

/// ROT-017 (map 01 R6): CoolProp's global `USE_GUESSES_IN_PROPSSI` makes most outputs throw. Here a guess is a
/// per-call seed, and every `Prop` reads the same with and without one, within `Flash` (a warm start is another
/// path, never `Exact`; VERIFICATION.md §11), errors included (`Undefined` Q, `NoModel`): Water's liquid, vapour
/// and supercritical states and Nitrogen's dense supercritical one, each from guesses at half and twice ρ. The
/// partial derivatives are every (of, wrt, at) of the twelve variables.
#[cfg(feature = "fluids-all")]
#[test]
fn a_guess_changes_no_output() {
    use DerivVar::*;
    let vars = [T, P, Dmolar, Dmass, Hmolar, Hmass, Smolar, Smass, Umolar, Umass, Gmolar, Gmass];
    let mut props: Vec<Prop> =
        product(&vars, &vars).flat_map(|(of, wrt)| vars.map(|at| Prop::Partial(Partial { of, wrt, at }))).collect();
    props.extend([
        Prop::T,
        Prop::P,
        Prop::Q,
        Prop::Dmolar,
        Prop::Dmass,
        Prop::Hmolar,
        Prop::Hmass,
        Prop::Smolar,
        Prop::Smass,
        Prop::Umolar,
        Prop::Umass,
        Prop::Gmolar,
        Prop::Gmass,
        Prop::Helmholtzmolar,
        Prop::Helmholtzmass,
        Prop::Cvmolar,
        Prop::Cvmass,
        Prop::Cpmolar,
        Prop::Cpmass,
        Prop::SpeedOfSound,
        Prop::Z,
        Prop::Cp0molar,
        Prop::Cp0mass,
        Prop::HmolarResidual,
        Prop::SmolarResidual,
        Prop::GmolarResidual,
        Prop::Bvirial,
        Prop::Cvirial,
        Prop::DBvirialDT,
        Prop::DCvirialDT,
        Prop::MolarMass,
        Prop::Viscosity,
        Prop::Conductivity,
        Prop::SurfaceTension,
    ]);
    for (name, p, t) in
        [("Water", 10e6, 300.0), ("Water", 1e4, 400.0), ("Water", 30e6, 700.0), ("Nitrogen", 10e6, 200.0)]
    {
        let fluid = embedded(name);
        let plain = fluid.state(pt(p, t)).unwrap();
        for factor in [0.5, 2.0] {
            let opts = FlashOptions::new().with_guess(t, factor * plain.rho(Basis::Molar));
            let warm = fluid.flash(pt(p, t), &opts).unwrap();
            for &prop in &props {
                let (a, b) = (fluid.prop(&plain, prop), fluid.prop(&warm, prop));
                assert!(same_output(&a, &b), "{name} {prop:?}: {a:?} and {b:?}");
            }
        }
    }
}

/// Whether two reads of one output agree: values within `Flash` (1e-9 of the larger), errors equal.
#[cfg(feature = "fluids-all")]
fn same_output(a: &Result<f64, Error>, b: &Result<f64, Error>) -> bool {
    match (a, b) {
        (Ok(a), Ok(b)) => (a - b).abs() <= 1e-9 * a.abs().max(b.abs()),
        (a, b) => a == b,
    }
}

/// ROT-080 (map 03 §6 "PT near saturation"): one rule at every T. CoolProp's PT of Water at its saturation pressure
/// answers liquid at 305 K (its p-based rule below 0.9 T_triple + 0.1 Tc) and throws at 315 K. Here, at 305 K and
/// 315 K alike, p = psat has two roots, and `Strict` returns both densities (ascending: ρ″, then ρ′), as does
/// `Stable` (at psat neither is more stable); `Nearest` and an imposed phase pick one; one float above or below
/// psat, the stable root is the only answer.
#[cfg(feature = "fluids-all")]
#[test]
fn pt_at_saturation_is_ambiguous_under_strict() {
    let water = embedded("Water");
    let curve = record_of("Water").superancillary_curve().unwrap();
    let with = |p, t, opts: FlashOptions| water.flash(pt(p, t), &opts);
    for t in [305.0, 315.0] {
        let sat = curve.at_t(t).unwrap();
        let p = sat.bubble.p;
        let Err(Error::Ambiguous { roots }) = water.state(pt(p, t)) else {
            panic!("{t} K: {:?}", water.state(pt(p, t)))
        };
        let &[vapour, liquid] = roots.as_slice() else { panic!("{roots:?}") };
        let near = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-9;
        assert!(near(liquid, sat.bubble.rho) && near(vapour, sat.dew.rho), "{t} K: {roots:?} against {sat:?}");
        assert_eq!(with(p, t, FlashOptions::new().with_roots(RootPolicy::Stable)), Err(Error::Ambiguous { roots }));
        let nearest = |x| with(p, t, FlashOptions::new().with_roots(RootPolicy::Nearest(x))).unwrap();
        let (l, v) = (nearest(1.1 * liquid), nearest(0.0));
        assert_eq!(
            (l.rho(Basis::Molar), l.phase(), v.rho(Basis::Molar), v.phase()),
            (liquid, Phase::Liquid, vapour, Phase::Gas)
        );
        let imposed = |phase| with(p, t, FlashOptions::new().with_phase(phase)).unwrap().rho(Basis::Molar);
        assert!(near(imposed(Phase::Liquid), liquid) && near(imposed(Phase::Gas), vapour), "{t} K");
        let (above, below) = (p.next_up(), p.next_down());
        let phase = |p| water.state(pt(p, t)).map(|s| s.phase());
        assert_eq!((phase(above), phase(below)), (Ok(Phase::Liquid), Ok(Phase::Gas)), "{t} K");
    }
}

/// `RootPolicy::Nearest` (PLAN.md M7.2), for PT's two roots at psat and PQ's temperatures alike: the root nearest
/// to x, of two equally near the smaller (at 2 and at 5.5 below), and none of none.
#[test]
fn nearest_takes_the_smaller_of_a_tie() {
    let roots = [1.0, 3.0, 8.0];
    let picks = [0.0, 1.9, 2.0, 2.1, 5.5, 5.6, 100.0].map(|x| nearest(x, &roots));
    assert_eq!(picks, [Some(1.0), Some(1.0), Some(1.0), Some(3.0), Some(3.0), Some(8.0), Some(8.0)]);
    assert_eq!(nearest(2.0, &[]), None);
}

/// map 01 §8 (an identity no oracle is needed for): (∂g/∂T)_p = −s for Water's PT state at 300 K and 1 atm, where
/// CoolProp 8.0.0 gives −393.0620684404547 J/kg/K (and −393.0620684404549 by the identity).
#[cfg(feature = "fluids-all")]
#[test]
fn water_dg_dt_at_constant_p_equals_minus_s() {
    let state = embedded("Water").state(pt(101_325.0, 300.0)).unwrap();
    let dg_dt = state.partial(Partial { of: DerivVar::Gmass, wrt: DerivVar::T, at: DerivVar::P }).unwrap();
    assert!((dg_dt / -state.s(Basis::Mass) - 1.0).abs() < 1e-14, "{dg_dt} and {}", state.s(Basis::Mass));
    assert!((dg_dt / -393.062_068_440_454_7 - 1.0).abs() < 1e-12, "{dg_dt}");
}

/// map 03 §6 "Inconsistent range policy": CoolProp's PT accepts Water at 3000 K and at 1e5 K (its Tmax is 2000 K)
/// and fails at 5 GPa inside its melting-line evaluator rather than on pmax. Here one domain rule serves every
/// pair: above Tmax or pmax PT is refused under `Enforce` with DT's and QT's errors, and flagged under
/// `Extrapolate`.
#[cfg(feature = "fluids-all")]
#[test]
fn pt_range_is_one_rule() {
    let water = embedded("Water");
    let hot = DomainError::AboveMaxTemperature { t: 3000.0, t_max: 2000.0 };
    assert_eq!((water.state(pt(1e6, 3000.0)), water.state(dt(40.0, 3000.0))), (Err(hot.into()), Err(hot.into())));
    let far = DomainError::AboveMaxTemperature { t: 1e5, t_max: 2000.0 };
    assert_eq!(water.state(pt(1e6, 1e5)), Err(far.into()));
    assert_eq!(water.state(pt(5e9, 1000.0)), Err(DomainError::AboveMaxPressure { p: 5e9, p_max: 1e9 }.into()));
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    let flagged = water.flash(pt(1e6, 3000.0), &extrapolate).unwrap();
    assert!(flagged.is_extrapolated() && flagged.phase() == Phase::SupercriticalGas, "{flagged:?}");
    assert!((flagged.p() / 1e6 - 1.0).abs() < 1e-15);
}
