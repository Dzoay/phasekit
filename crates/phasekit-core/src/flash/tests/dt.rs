//! DT and the phase rule: the domain, the dome as the curve or the EOS's VLE decides it, the model's critical
//! point, the acceptance gate and the density bands.

use super::*;

/// D6: below the triple point the default refuses; `Extrapolate` evaluates a metastable single-phase
/// state and flags it (scalar and batch); a saturation fit is never evaluated outside its range, where the generic
/// VLE of the EOS decides instead (M7.2).
#[test]
fn extrapolation_is_opt_in_flagged_and_never_extends_a_fit() {
    let mut record = FluidRecord::synthetic("X").unwrap();
    record.limits = Limits::new(169.0, 420.0, 100e6).unwrap().with_t_triple(175.0);
    let fluid = Fluid::new(Arc::new(record.builder().unwrap().saturation(Fit).build()));
    let dt = |t| Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(t).unwrap());
    let enforce = FlashOptions::new();
    let extrapolate = enforce.with_domain(DomainPolicy::Extrapolate);
    let liquid = |opts: FlashOptions| opts.with_phase(Phase::Liquid);

    let below_triple = DomainError::BelowMinTemperature { t: 172.0, t_min: 175.0 };
    assert_eq!(fluid.flash(dt(172.0), &liquid(enforce)), Err(below_triple.into()));
    assert!(fluid.flash(dt(172.0), &liquid(extrapolate)).unwrap().is_extrapolated());
    assert!(!fluid.flash(dt(300.0), &liquid(extrapolate)).unwrap().is_extrapolated());
    // Without a hint, outside 200-380 K the fit is not evaluated under either policy (it panics if it is): the
    // generic VLE of the EOS is asked, and the toy EOS has no loop there.
    for (t, opts) in [(172.0, extrapolate), (190.0, enforce), (190.0, extrapolate)] {
        assert_eq!(fluid.flash(dt(t), &opts), Err(Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 }));
    }
    assert_eq!(fluid.flash(dt(300.0), &enforce).unwrap().phase(), Phase::Liquid);

    // The flag survives the batch driver as a per-cell status that is not a failure.
    let (x, y) = ([5_000.0; 2], [172.0, 300.0]);
    let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &[Prop::P]).with_flash(liquid(extrapolate));
    let (mut out, mut status) = ([0.0; 2], [Status::Ok; 2]);
    let summary = batch::evaluate(&fluid, &req, &mut out, &mut status).unwrap();
    assert_eq!((status, summary.failed_cells), ([Status::Extrapolated, Status::Ok], 0));

    // Real Water (PLAN.md M5.3): supercooled liquid at 260 K is refused by default and flagged under `Extrapolate`
    // with a liquid hint. Without a hint the superancillary, which starts at 273.16 K, is not evaluated: the EOS's
    // own VLE gives psat 222.6 Pa there (M7.2), and 55 000 mol/m³ lies just inside its dome, 55 500 above it.
    #[cfg(feature = "fluids-all")]
    {
        let water = embedded("Water");
        let input = Input::dt(Density::molar(55_000.0).unwrap(), Temperature::new(260.0).unwrap());
        let below = DomainError::BelowMinTemperature { t: 260.0, t_min: 273.16 };
        assert_eq!(water.flash(input, &enforce), Err(below.into()));
        let supercooled = water.flash(input, &liquid(extrapolate)).unwrap();
        assert!(supercooled.is_extrapolated() && supercooled.phase() == Phase::Liquid);
        let wet = water.flash(input, &extrapolate).unwrap();
        assert_eq!((wet.phase(), wet.path().strategy, wet.is_extrapolated()), (Phase::TwoPhase, Strategy::Vle, true));
        assert!(wet.quality().is_some_and(|q| q < 1e-7) && (wet.p() / 222.557 - 1.0).abs() < 1e-5, "{wet:?}");
        let denser = Input::dt(Density::molar(55_500.0).unwrap(), Temperature::new(260.0).unwrap());
        assert_eq!(water.flash(denser, &extrapolate).map(|s| s.phase()), Ok(Phase::Liquid));
    }
}

/// ROT-073 (map 03 §6): CoolProp's imposed two-phase DT of Water at 1000 mol/m³ and 400 K throws "rhomolar is less
/// than zero" (stale SatL/SatV). Here it is the lever rule on the saturation at T: QT's state at its quality, and
/// the same as without the hint, since the phase rule finds the dome too. On the dome's edge (M7.9) a density
/// within the acceptance gate's share (1e-9) of a saturated one is that side, Q 0 or 1 exactly, and one 1e-6
/// beyond it is refused (a quality outside [0, 1]).
#[cfg(feature = "fluids-all")]
#[test]
fn imposed_two_phase_dt_matches_qt() {
    let (water, two_phase) = (embedded("Water"), FlashOptions::new().with_phase(Phase::TwoPhase));
    let imposed = water.flash(dt(1000.0, 400.0), &two_phase).unwrap();
    let qt = water.state(q_t(imposed.quality().unwrap(), 400.0)).unwrap();
    let m = Basis::Molar;
    let pairs = [(imposed.p(), qt.p()), (imposed.h(m), qt.h(m)), (imposed.s(m), qt.s(m)), (imposed.rho(m), qt.rho(m))];
    for (got, want) in pairs {
        assert!((got - want).abs() <= 1e-12 * want.abs(), "{got} against {want}");
    }
    assert_eq!(imposed, water.state(dt(1000.0, 400.0)).unwrap());
    for (q, outwards) in [(0.0, 1.0), (1.0, -1.0)] {
        let rho = water.state(q_t(q, 400.0)).unwrap().rho(m);
        let at = |share: f64| water.flash(dt(rho * (1.0 + outwards * share), 400.0), &two_phase);
        assert!(at(1e-10).is_ok_and(|s| s.phase() == Phase::TwoPhase && s.quality() == Some(q)), "{:?}", at(1e-10));
        assert!(matches!(at(1e-6), Err(Error::InvalidInput { quantity: "quality", .. })), "{:?}", at(1e-6));
    }
}

/// ROT-078, DIV-0012 (map 12 §6.3, #3394): Water at 250 K, below its 273.16 K triple point, is refused; CoolProp
/// accepts DT(55018.5 mol/m³, 250 K) and returns p = −5.93 Pa.
#[cfg(feature = "fluids-all")]
#[test]
fn dt_below_the_triple_point_is_refused() {
    let below = DomainError::BelowMinTemperature { t: 250.0, t_min: 273.16 };
    assert_eq!(embedded("Water").state(dt(55_018.5, 250.0)), Err(below.into()));
}

/// D6 (map 03 §6, map 01 R12): `Extrapolate` belongs to the call. The same handle refuses the same state on the
/// next call with the default options, and a hint given once is not remembered.
#[cfg(feature = "fluids-all")]
#[test]
fn extrapolate_is_per_call_never_sticky() {
    let water = embedded("Water");
    let extrapolate = FlashOptions::new().with_phase(Phase::Liquid).with_domain(DomainPolicy::Extrapolate);
    let below = Err(DomainError::BelowMinTemperature { t: 265.0, t_min: 273.16 }.into());
    for _ in 0..2 {
        assert!(water.flash(dt(55_000.0, 265.0), &extrapolate).unwrap().is_extrapolated());
        assert_eq!(water.state(dt(55_000.0, 265.0)), below);
        assert_eq!(water.state(dt(55_400.0, 300.0)).unwrap().phase(), Phase::Liquid); // above ρ′ = 55,315
    }
}

/// User decision 4, map 04 U4 (PLAN.md M6.9): below its critical temperature a pseudo-pure fluid's phase rule is
/// the pure VLE of its EOS, seeded by its density ancillaries, as CoolProp's (`HelmholtzEOSMixtureBackend.cpp:
/// 2447-2507`): R410A at 280 K is gas, two-phase on that VLE's densities and pressure, or liquid; above Tc it is
/// supercritical.
#[cfg(feature = "fluids-all")]
#[test]
fn pseudo_pure_subcritical_dt_follows_its_eos_vle() {
    let r410a = embedded("R410A");
    let phases = [100.0, 5_000.0, 16_000.0].map(|rho| r410a.state(dt(rho, 280.0)).map(|s| s.phase()));
    assert_eq!(phases, [Ok(Phase::Gas), Ok(Phase::TwoPhase), Ok(Phase::Liquid)]);
    let record = record_of("R410A");
    let definition = record.pseudo_pure.as_ref().unwrap();
    let seeds = (definition.rho_l.at(280.0), definition.rho_v.at(280.0));
    let vle = crate::vle::at_t(r410a.model().helmholtz().unwrap(), 280.0, seeds).unwrap();
    let state = r410a.state(dt(5_000.0, 280.0)).unwrap();
    assert_eq!((state.p(), state.path().strategy), (vle.dew.p, Strategy::Vle));
    assert_eq!(r410a.state(dt(5_000.0, 400.0)).unwrap().phase(), Phase::Supercritical);
}

/// ROT-020: the state names the strategy that produced it: `Direct` for a single phase, `Superancillary` in the
/// dome, where the curve's equilibrium is the answer.
#[cfg(feature = "fluids-all")]
#[test]
fn solve_path_names_the_winning_strategy() {
    let water = embedded("Water");
    let strategy = |rho, t| water.state(dt(rho, t)).unwrap().path();
    let direct = SolvePath { strategy: Strategy::Direct, iterations: 0 };
    assert_eq!([strategy(10.0, 700.0), strategy(10.0, 400.0), strategy(55_400.0, 300.0)], [direct; 3]);
    assert_eq!(strategy(1_000.0, 400.0), SolvePath { strategy: Strategy::Superancillary, iterations: 0 });
}

/// PLAN.md M6.6 (E14; ROT-088, ROT-096): an EOS edit other than R or ρ_r (here Nitrogen's first power-term
/// coefficient, times 1 + 1e-6) leaves its superancillary stale, a `Guess`. DT inside the dome then only seeds the
/// pure VLE with it: the two-phase state's pressure and quality are the edited EOS's own saturation, bit for bit
/// that of `vle::at_t` from the same seeds, and the stale curve's pressure is 1e-7 or more away.
#[cfg(feature = "fluids-all")]
#[test]
fn stale_curve_is_a_guess_polished_by_vle() {
    let mut record = record_of("Nitrogen");
    record.eos.power.first_mut().unwrap().n *= 1.0 + 1e-6;
    assert_eq!(record.superancillary_freshness(), Some(crate::data::SaFreshness::Stale));
    let stale = record.superancillary_curve().unwrap();
    assert_eq!(stale.accuracy(), SatAccuracy::Guess);
    let curve_of = record.clone();
    let pure = record.builder().unwrap().lazy_saturation(move || Ok(curve_of.superancillary_curve())).build();
    let t = 100.0;
    let guess = stale.at_t(t).unwrap();
    let vle = crate::vle::at_t(pure.eos(), t, (guess.bubble.rho, guess.dew.rho)).unwrap();
    let fluid = Fluid::new(Arc::new(pure));
    let rho = crate::num::math::sqrt(vle.bubble.rho * vle.dew.rho);
    let state = fluid.state(dt(rho, t)).unwrap();
    assert_eq!((state.phase(), state.p().to_bits()), (Phase::TwoPhase, vle.dew.p.to_bits()));
    assert_eq!(state.path().strategy, Strategy::Vle);
    let q = (1.0 / rho - 1.0 / vle.bubble.rho) / (1.0 / vle.dew.rho - 1.0 / vle.bubble.rho);
    assert_eq!(state.quality().map(f64::to_bits), Some(q.to_bits()));
    assert!((guess.bubble.p / vle.dew.p - 1.0).abs() > 1e-7, "the edit moved saturation: {guess:?} {vle:?}");
    // PT polishes the stale curve too: between its pressure and the VLE's, the VLE decides the phase (M7.1).
    let between = 0.5 * (guess.bubble.p + vle.dew.p);
    let want = if between > vle.dew.p { Phase::Liquid } else { Phase::Gas };
    let input = Input::pt(Pressure::new(between).unwrap(), Temperature::new(t).unwrap());
    assert_eq!(fluid.state(input).map(|s| s.phase()), Ok(want));
}

/// ROT-092 (map 03 §6): a DT state in the dome reads the exact curve, no VLE: its pressure and phase densities are
/// the curve's, bit for bit. The pressure check uses that saturation pressure: at 30,000 mol/m³ and 400 K the
/// single-phase EOS gives 2.2e11 Pa, far above pmax, yet the state is a valid two-phase one.
#[cfg(feature = "fluids-all")]
#[test]
fn dt_in_dome_reads_the_curve_not_vle() {
    let state = embedded("Water").state(dt(30_000.0, 400.0)).unwrap();
    let sat = record_of("Water").superancillary_curve().unwrap().at_t(400.0).unwrap();
    assert_eq!((state.phase(), state.p().to_bits()), (Phase::TwoPhase, sat.bubble.p.to_bits()));
    let q = (1.0 / 30_000.0 - 1.0 / sat.bubble.rho) / (1.0 / sat.dew.rho - 1.0 / sat.bubble.rho);
    assert_eq!(state.quality().map(f64::to_bits), Some(q.to_bits()));
}

/// A curve whose bubble and dew sides differ (a pseudo-pure fluid's, `Definition`) over [300 K, 600 K]: p′ 2.0 MPa
/// and p″ 1.9 MPa, ρ′ 52 000 and ρ″ 80 mol/m³ at every T (near Water's own at 400 K: the VLE's seeds).
#[cfg(feature = "fluids-all")]
#[derive(Debug)]
struct Blend;

#[cfg(feature = "fluids-all")]
impl SaturationCurve for Blend {
    fn accuracy(&self) -> SatAccuracy {
        SatAccuracy::Definition
    }
    fn t_range(&self) -> (f64, f64) {
        (300.0, 600.0)
    }
    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        Ok(SatPair { bubble: SatSide { t, p: 2.0e6, rho: 52_000.0 }, dew: SatSide { t, p: 1.9e6, rho: 80.0 } })
    }
    fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
        Err(Error::Unsupported { pair: Pair::PQ })
    }
}

/// D4 (map 04 U4; PLAN.md M6.9): a curve whose bubble and dew sides differ, a `Definition`, only seeds the EOS's
/// pure VLE inside its dome: Water's EOS under such a curve at 400 K is two-phase on Water's own saturation there,
/// path `Vle`, and gas below its dew density.
#[cfg(feature = "fluids-all")]
#[test]
fn a_definitions_dome_is_its_eos_vle() {
    let blend = Fluid::new(Arc::new(record_of("Water").builder().unwrap().saturation(Blend).build()));
    let state = blend.state(dt(1_000.0, 400.0)).unwrap();
    let water = embedded("Water").state(dt(1_000.0, 400.0)).unwrap();
    assert_eq!((state.phase(), state.path().strategy), (Phase::TwoPhase, Strategy::Vle));
    assert!((state.p() / water.p() - 1.0).abs() < 1e-12, "{} against {}", state.p(), water.p());
    assert_eq!(blend.state(dt(5.0, 400.0)).map(|s| s.phase()), Ok(Phase::Gas));
}

/// The model's critical point labels states (map 03 §3.3): R114's published point (418.83 K, 3.257 MPa) lies below
/// its EOS's, the top of its superancillary (420.61 K, 3.352 MPa), so at 419.5 K the dome is still there: a state
/// between its densities is two-phase, as CoolProp says with superancillaries on, not supercritical.
#[cfg(feature = "fluids-all")]
#[test]
fn phase_labels_use_the_models_critical_point() {
    let record = record_of("R114");
    let published = record.critical.unwrap();
    let curve = record.superancillary_curve().unwrap();
    let (t, t_c) = (419.5, curve.t_range().1);
    assert!(published.t < t && t < t_c, "{} < {t} < {t_c}", published.t);
    let sat = curve.at_t(t).unwrap();
    let state = embedded("R114").state(dt((sat.bubble.rho + sat.dew.rho) / 2.0, t)).unwrap();
    assert_eq!(state.phase(), Phase::TwoPhase);
    assert_eq!(embedded("R114").state(dt(sat.dew.rho / 2.0, t)).unwrap().phase(), Phase::Gas);
}

/// The critical point labels a state only at both of the model's coordinates, the top of Water's exact curve; off
/// it, at T_c or on the critical isochore, the pressure decides between supercritical and supercritical gas.
#[cfg(feature = "fluids-all")]
#[test]
fn critical_point_needs_both_coordinates() {
    let curve = record_of("Water").superancillary_curve().unwrap();
    let t_c = curve.t_range().1;
    let rho_c = curve.at_t(t_c).unwrap().bubble.rho;
    let water = embedded("Water");
    let at = |rho: f64, t: f64| water.state(dt(rho, t)).map(|s| s.phase());
    assert_eq!(at(rho_c, t_c), Ok(Phase::CriticalPoint));
    assert_eq!(at(1.2 * rho_c, t_c), Ok(Phase::Supercritical));
    assert_eq!(at(0.8 * rho_c, t_c), Ok(Phase::SupercriticalGas));
    assert_eq!(at(rho_c, 1.01 * t_c), Ok(Phase::Supercritical));
}

/// An exact curve over [300 K, `t`] whose top, the model's critical point, is (`t`, `p`, `rho`), with liquid at
/// `rho_l` and vapour at `rho_v` below it: it puts Water's critical point where a test needs it.
#[cfg(feature = "fluids-all")]
#[derive(Debug)]
struct Top {
    t: f64,
    p: f64,
    rho: f64,
    rho_l: f64,
    rho_v: f64,
}

#[cfg(feature = "fluids-all")]
impl SaturationCurve for Top {
    fn accuracy(&self) -> SatAccuracy {
        SatAccuracy::Exact
    }
    fn t_range(&self) -> (f64, f64) {
        (300.0, self.t)
    }
    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        let (bubble, dew) = if t == self.t { (self.rho, self.rho) } else { (self.rho_l, self.rho_v) };
        Ok(SatPair { bubble: SatSide { t, p: self.p, rho: bubble }, dew: SatSide { t, p: self.p, rho: dew } })
    }
    fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
        Err(Error::Unsupported { pair: Pair::PQ })
    }
}

/// Water's EOS under the curve `top`.
#[cfg(feature = "fluids-all")]
fn water_under(top: Top) -> Fluid {
    Fluid::new(Arc::new(record_of("Water").builder().unwrap().saturation(top).build()))
}

/// The model's critical point where its solve cannot run (map 03 §6): under an exact curve whose top is Water's
/// reducing point, τ = δ = 1, where the non-analytic terms are singular, it is that top, with origin `Model`.
#[cfg(feature = "fluids-all")]
#[test]
fn an_unsolvable_critical_point_is_the_exact_curves_top() {
    let eos = record_of("Water").eos;
    let (t, rho) = (eos.t_reducing, eos.rho_reducing);
    let model = water_under(Top { t, p: 22.064e6, rho, rho_l: 20_000.0, rho_v: 10_000.0 }).model().critical_point();
    let origin = crate::model::CriticalOrigin::Model;
    assert_eq!(model, Some(crate::model::CriticalPoint { t, p: 22.064e6, rho, origin }));
}

/// D6: the acceptance gate runs on every single-phase state the phase rule chose but the critical point, where
/// (∂p/∂ρ)_T = 0 by definition. Under a curve whose top is 600 K, at 610 K the rule says supercritical, yet inside
/// Water's own spinodal there (∂p/∂ρ)_T < 0 and the state is refused; the curve's top, put inside the spinodal at
/// 600 K, is the critical point and a state.
#[cfg(feature = "fluids-all")]
#[test]
fn acceptance_gate_skips_only_the_critical_point() {
    let dp_drho = |rho: f64, t: f64| {
        imposed(rho, t).partial(Partial { of: DerivVar::P, wrt: DerivVar::Dmolar, at: DerivVar::T }).unwrap()
    };
    let spinodal = |t: f64| (20..160).map(|k| 250.0 * f64::from(k)).find(|&rho| dp_drho(rho, t) < 0.0).unwrap();
    let (rho_600, rho_610) = (spinodal(600.0), spinodal(610.0));
    let top = Top { t: 600.0, p: imposed(rho_600, 600.0).p(), rho: rho_600, rho_l: 40_000.0, rho_v: 1_000.0 };
    let fluid = water_under(top);
    assert_eq!(fluid.state(dt(rho_600, 600.0)).map(|s| s.phase()), Ok(Phase::CriticalPoint));
    assert_eq!(fluid.state(dt(rho_610, 610.0)), Err(DomainError::MechanicallyUnstable.into()));
}

/// The pressure that splits supercritical from supercritical gas, and liquid from supercritical liquid, is the
/// critical pressure itself: a state exactly at p_c is not above it. Water under curves whose top pressure is a
/// chosen state's own.
#[cfg(feature = "fluids-all")]
#[test]
fn a_state_at_the_critical_pressure_is_not_above_it() {
    let top = |p| Top { t: 650.0, p, rho: 17_000.0, rho_l: 50_000.0, rho_v: 100.0 };
    let (rho, t) = (5_000.0, 700.0);
    let fluid = water_under(top(imposed(rho, t).p()));
    assert_eq!(fluid.state(dt(rho, t)).map(|s| s.phase()), Ok(Phase::SupercriticalGas));
    assert_eq!(fluid.state(dt(1.01 * rho, t)).map(|s| s.phase()), Ok(Phase::Supercritical));
    let (rho, t) = (54_000.0, 400.0);
    let fluid = water_under(top(imposed(rho, t).p()));
    assert_eq!(fluid.state(dt(rho, t)).map(|s| s.phase()), Ok(Phase::Liquid));
    assert_eq!(fluid.state(dt(1.001 * rho, t)).map(|s| s.phase()), Ok(Phase::SupercriticalLiquid));
}

/// The acceptance gate (D6): a single-phase state needs (∂p/∂ρ)_T > 0 and cv > 0; an infinite cv passes.
#[test]
fn acceptance_gate_needs_mechanical_stability() {
    let stable = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
    let r = 8.314_462_618;
    assert_eq!(accept(r, 300.0, &stable), Ok(()));
    assert_eq!(accept(r, 300.0, &Bundle { a20: f64::NEG_INFINITY, ..stable }), Ok(()));
    let unstable = Err(DomainError::MechanicallyUnstable.into());
    assert_eq!(accept(r, 300.0, &Bundle { a02: -2.0, ..stable }), unstable); // (∂p/∂ρ)_T = 0
    assert_eq!(accept(r, 300.0, &Bundle { a20: 0.0, ..stable }), unstable); // cv = 0
}

/// The density bands (user decision PP3; `HelmholtzEOSMixtureBackend.cpp:2340-2380`) on a synthetic pair, ρ′ 1000
/// and ρ″ 100 mol/m³, p′ 1 MPa: gas below 0.95·ρ″ (not at it), liquid above 1.05·ρ′ (not at it); between, the strip
/// within 0.9975 of either side asks for stability, and nothing else does; in the liquid strip, liquid when the
/// ancillary quality is below 0.01, p above 1.05·p′ (not at it) and the state stable. Near the top (ρ″ 990) the
/// liquid strip's quality exceeds 0.01.
#[test]
fn bands_are_coolprops() {
    let pair = |rho_v: f64| {
        let side = |p, rho| SatSide { t: 300.0, p, rho };
        SatPair { bubble: side(1e6, 1_000.0), dew: side(0.98e6, rho_v) }
    };
    let sat = pair(100.0);
    let mut asked = 0;
    let mut at = |rho: f64, p: f64, stable: bool| {
        band(rho, p, &sat, || {
            asked += 1;
            stable
        })
    };
    assert_eq!((at(0.95 * 100.0 - 1e-9, 1e6, true), at(0.95 * 100.0, 1e6, true)), (Some(Phase::Gas), None));
    assert_eq!((at(1.05 * 1_000.0 + 1e-9, 1e6, true), at(1.05 * 1_000.0, 1e6, true)), (Some(Phase::Liquid), None));
    assert_eq!((at(500.0, 2e6, true), at(99.8, 2e6, true), at(997.4, 2e6, true)), (None, None, None));
    assert_eq!(at(99.7, 2e6, true), None, "the vapour strip: quality above 1");
    let edge: f64 = 0.95 * (1.05 * 1_000.0);
    let above = edge.next_up();
    assert_eq!((at(edge, 2e6, true), at(above, 2e6, true)), (None, Some(Phase::Liquid)), "the strip's edge");
    assert_eq!([at(998.0, 2e6, true), at(998.0, 2e6, false)], [Some(Phase::Liquid), None]);
    assert_eq!([at(998.0, 1.05 * 1e6, true), at(998.0, 1.04e6, true)], [None, None]);
    assert_eq!(asked, 3, "asked only in the liquid strip with the quality and p met");
    let near_top = pair(990.0);
    assert_eq!(band(998.0, 2e6, &near_top, || true), None, "quality 0.2 in the liquid strip");
    let side = |p, rho| SatSide { t: 300.0, p, rho };
    let scaled = SatPair { bubble: side(1e6, 1.0), dew: side(0.98e6, 0.1) };
    assert_eq!(band(0.998, 2e6, &scaled, || true), Some(Phase::Liquid), "the same bands 1000 times thinner");
}

/// The liquid strip's rule and the stability it asks, exactly: a quality of 0.01 or p at 1.05·p′ is not liquid, and
/// stability needs both pressure derivatives positive (a zero, a negative or no value is unstable).
#[test]
fn strip_rule_and_stability_are_exact() {
    let liquid = |q: f64, p: f64, stable: bool| strip_liquid(q, p, 1e6, || stable);
    let (below, above) = (0.01_f64.next_down(), (1.05 * 1e6_f64).next_up());
    assert!(liquid(below, 2e6, true) && liquid(0.0, above, true));
    assert!(!liquid(0.01, 2e6, true) && !liquid(0.0, 1.05 * 1e6, true) && !liquid(0.0, 2e6, false));
    assert!(stable(Some([1.0, 1.0])));
    for c in [Some([0.0, 1.0]), Some([1.0, 0.0]), Some([-1.0, 1.0]), Some([1.0, -1.0]), None] {
        assert!(!stable(c), "{c:?}");
    }
}

/// PP3 on real blends: R407C's EOS has no dome at 358 K (its critical point is 356.60 K, the published one 359.345
/// K), where the bands alone decide gas and liquid (above pc a supercritical liquid), as CoolProp's; between them
/// its VLE fails. An imposed `TwoPhase` skips the bands: R410A at 280 K and 100 mol/m³, gas by the bands, is
/// refused (a quality outside [0, 1]).
#[cfg(feature = "fluids-all")]
#[test]
fn bands_decide_where_the_eos_has_no_dome() {
    let r407c = embedded("R407C");
    let phase = |rho: f64| r407c.state(dt(rho, 358.0)).map(|s| s.phase());
    assert_eq!((phase(100.0), phase(12_000.0)), (Ok(Phase::Gas), Ok(Phase::SupercriticalLiquid)));
    assert!(phase(5_300.0).is_err(), "no dome on the EOS: {:?}", phase(5_300.0));
    let r410a = embedded("R410A");
    let imposed = FlashOptions::new().with_phase(Phase::TwoPhase);
    assert!(r410a.flash(dt(100.0, 280.0), &imposed).is_err());
    assert_eq!(r410a.state(dt(100.0, 280.0)).map(|s| s.phase()), Ok(Phase::Gas));
}

/// The generic VLE runs only below the labelling critical temperature (PLAN.md M7.2): Water's at 0.9 Tc is a
/// saturation, and at Tc, above it, or without a critical point the pair has none there (`Unsupported`), never a VLE
/// started where it has no solution.
#[cfg(feature = "fluids-all")]
#[test]
fn generic_vle_stops_at_the_critical_temperature() {
    let water = record_of("Water").compile().unwrap();
    let crit = Some((647.096, 22.064e6, 322.0 / 0.018_015_268));
    assert!(generic_vle(&water, crit, 0.9 * 647.096, Pair::DT).is_ok());
    for (crit, t) in [(crit, 647.096), (crit, 1.01 * 647.096), (None, 0.9 * 647.096)] {
        let refused = generic_vle(&water, crit, t, Pair::DT);
        assert!(matches!(refused, Err(Error::Unsupported { pair: Pair::DT })), "{t} K: {refused:?}");
    }
}
