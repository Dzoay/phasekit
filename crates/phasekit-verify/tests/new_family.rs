//! Acceptance gate for D3 (Lean graft; the systems and rust judges' fatal flaw): a model family defined
//! OUTSIDE every first-party crate reaches the registry, the typed flash, derivative outputs, reference
//! states, the batch driver, the compat strings and 16 threads, with zero core edits. The M11 cubic family
//! must pass the same gate with a zero-line diff in `phasekit-core` (Extensible graft).
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::num::NonZeroUsize;
use std::sync::Arc;

use phasekit_compat::{CompatError, props_si_in};
use phasekit_core::batch::{self, BatchRequest, ExecPolicy, Status};
use phasekit_core::{
    Basis, CriticalOrigin, CriticalPoint, DataTerms, Density, DerivVar, Derivs, Enthalpy, Entropy, Error, FlashOptions,
    FluidInfo, HelmholtzModel, Input, Limits, ModelKey, Order, Pair, Partial, Phase, Pressure, Prop, PureFluid,
    ReferenceState, Registry, SatAccuracy, SatPair, SatSide, SaturationCurve, Source, Strategy, Temperature,
    ThermoModel, Virials, math,
};
use phasekit_verify::{fd_first_order, gauge_invariance, policy_equivalence};

const R: f64 = 8.314_462_618;

/// van der Waals as a family crate would write it: a closed-form, reducing-invariant bundle in (T, ρ).
#[derive(Debug)]
struct VanDerWaals {
    a: f64,
    b: f64,
}

impl VanDerWaals {
    fn critical(&self) -> CriticalPoint {
        let (a, b) = (self.a, self.b);
        CriticalPoint {
            t: 8.0 * a / (27.0 * R * b),
            p: a / (27.0 * b * b),
            rho: 1.0 / (3.0 * b),
            origin: CriticalOrigin::Model,
        }
    }
}

impl HelmholtzModel for VanDerWaals {
    fn gas_constant(&self) -> f64 {
        R
    }
    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        let x = self.b * rho;
        let w = x / (1.0 - x);
        let f2 = -self.a * rho / (R * t);
        Derivs::from_fn(order, |i, j| match (i, j) {
            (0, 0) => -math::ln(1.0 - x) + f2,
            (1, 0) | (1, 1) => f2,
            (0, 1) => w + f2,
            (0, 2) => w * w,
            (0, 3) => 2.0 * w * w * w,
            (0, 4) => 6.0 * w * w * w * w,
            _ => 0.0,
        })
    }
    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
        // Monatomic ideal gas: α⁰ = ln ρ + 1.5 ln τ with τ = 1/T.
        Derivs::from_fn(order, |i, j| match (i, j) {
            (0, 0) => math::ln(rho) - 1.5 * math::ln(t),
            (1, 0) => 1.5,
            (2, 0) => -1.5,
            (3, 0) => 3.0,
            (4, 0) => -9.0,
            (0, j) => Derivs::IDEAL_DELTA.get(0, j).unwrap_or(0.0),
            _ => 0.0,
        })
    }
    fn rho_max(&self, _t: f64) -> f64 {
        0.999 / self.b
    }
    /// α^r = bρ + b²ρ²/2 − aρ/(RT) + …, so B = b − a/(RT) and C = b², exactly (E4).
    fn zero_density(&self, t: f64) -> Option<Virials> {
        let (a, b) = (self.a, self.b);
        Some(Virials { b: b - a / (R * t), c: b * b, db_dt: a / (R * t * t), dc_dt: 0.0 })
    }
}

fn vdw() -> (Arc<dyn ThermoModel>, CriticalPoint) {
    let model = VanDerWaals { a: 0.1355, b: 3.2e-5 };
    let crit = model.critical();
    let source = Source::new("vanderWaals-1873", None, DataTerms::Published);
    let key = ModelKey::from_content(b"vdw a=0.1355 b=3.2e-5");
    let info = FluidInfo::new("vdW-Argon", 0.039_948, source, key).unwrap().with_aliases(&["vdw"]);
    let limits = Limits::new(50.0, 2000.0, 1e9).unwrap();
    let package = PureFluid::builder(info, model, limits).critical(crit).build();
    (Arc::new(package), crit)
}

fn dt(rho: f64, t: f64) -> Input {
    Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())
}

#[test]
fn registry_and_flash_reach_the_new_family() {
    let (model, crit) = vdw();
    let reg = Registry::embedded().unwrap().with_model(model).unwrap(); // shares the embedded layer
    assert_eq!(reg.canonical_name("h2o"), Some("Water"));
    let fluid = reg.get("VDW").unwrap();
    let (t, rho) = (2.0 * crit.t, 100.0);
    let state = fluid.flash(dt(rho, t), &FlashOptions::default()).unwrap();
    let want = rho * R * t / (1.0 - 3.2e-5 * rho) - 0.1355 * rho * rho;
    assert!((state.p() / want - 1.0).abs() < 1e-14);
    assert_eq!((state.phase(), state.path().strategy), (Phase::SupercriticalGas, Strategy::Direct));
    assert!((state.cv(Basis::Molar).unwrap() - 1.5 * R).abs() < 1e-12);
    fd_first_order(fluid.model().helmholtz().unwrap(), t, rho, 1e-6, 1e-8).unwrap();
    // Undeclared pairs are refused, never `todo!`.
    let hs = Input::new(Pair::HS, 1e3, 10.0, Basis::Molar).unwrap();
    assert_eq!(fluid.flash(hs, &FlashOptions::default()).unwrap_err(), Error::Unsupported { pair: Pair::HS });
}

/// S-10 / E18, PLAN.md M7.2: below Tc a family with no saturation curve gets the generic VLE of its EOS, seeded by the
/// core's own Maxwell construction between the spinodals, never a load error or a guess. Arbiter: the test's Maxwell
/// construction (bisected to adjacent floats), at 0.8 and 0.4 Tc: inside the dome DT is two-phase at its pressure with
/// the lever rule's quality (`Strategy::Vle`); PT at that pressure has both roots, its densities; an imposed single
/// phase is honoured.
#[test]
fn subcritical_without_a_curve_uses_the_eos_vle() {
    let (model, crit) = vdw();
    let reg = Registry::empty().with_model(model).unwrap();
    let fluid = reg.get("vdw").unwrap();
    for t in [0.8 * crit.t, 0.4 * crit.t] {
        let sat = maxwell(0.1355, 3.2e-5, t);
        let (p_sat, rho_l, rho_v) = (sat.bubble.p, sat.bubble.rho, sat.dew.rho);
        let rho = 1.0 / (0.75 / rho_l + 0.25 / rho_v);
        let state = fluid.state(dt(rho, t)).unwrap();
        assert_eq!((state.phase(), state.path().strategy), (Phase::TwoPhase, Strategy::Vle), "{t} K");
        assert!((state.p() / p_sat - 1.0).abs() < 1e-12, "{t} K: {} against {p_sat}", state.p());
        assert!((state.quality().unwrap() - 0.25).abs() < 1e-10, "{t} K: {:?}", state.quality());
        let pt = Input::pt(Pressure::new(state.p()).unwrap(), Temperature::new(t).unwrap());
        let Err(Error::Ambiguous { roots }) = fluid.state(pt) else { panic!("{t} K: {:?}", fluid.state(pt)) };
        let near = |got: f64, want: f64| (got / want - 1.0).abs() < 1e-10;
        assert!(near(roots.as_slice()[0], rho_v) && near(roots.as_slice()[1], rho_l), "{t} K: {roots:?}");
        let gas = fluid.flash(dt(rho, t), &FlashOptions::new().with_phase(Phase::Gas)).unwrap();
        assert_eq!(gas.phase(), Phase::Gas);
    }
}

/// The van der Waals saturation curve by a Maxwell construction ([`maxwell`]), as the family crate would ship it, over
/// [0.5 Tc, Tc].
#[derive(Debug)]
struct Maxwell {
    a: f64,
    b: f64,
}

impl Maxwell {
    fn p(&self, t: f64, rho: f64) -> f64 {
        rho * R * t / (1.0 - self.b * rho) - self.a * rho * rho
    }
    /// g/(RT) up to a function of T alone: ln ρ + α^r + p/(ρRT).
    fn g(&self, t: f64, rho: f64) -> f64 {
        let x = self.b * rho;
        math::ln(rho / (1.0 - x)) + 1.0 / (1.0 - x) - 2.0 * self.a * rho / (R * t)
    }
}

/// The root of `f` between `lo` (f < 0) and `hi` (f ≥ 0), bisected to adjacent floats.
fn bisect(f: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    loop {
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            return mid;
        }
        if f(mid) < 0.0 { lo = mid } else { hi = mid }
    }
}

impl SaturationCurve for Maxwell {
    fn accuracy(&self) -> SatAccuracy {
        SatAccuracy::Exact
    }
    fn t_range(&self) -> (f64, f64) {
        let t_c = 8.0 * self.a / (27.0 * R * self.b);
        (0.5 * t_c, t_c)
    }
    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        let (t_lo, t_c) = self.t_range();
        assert!((t_lo..=t_c).contains(&t), "the core evaluated the curve outside its range at {t} K");
        Ok(maxwell(self.a, self.b, t))
    }
    fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
        Err(Error::Unsupported { pair: Pair::PQ })
    }
}

/// The van der Waals saturation at `t` ≤ Tc by a Maxwell construction: p and the molar Gibbs energy equal on both sides.
/// p_sat bisects on g′ − g″ between the spinodal pressures, and each side's density bisects p(ρ) = p_sat between its
/// spinodal and its bound, every bisection down to adjacent floats.
fn maxwell(a: f64, b: f64, t: f64) -> SatPair {
    let model = Maxwell { a, b };
    let (rho_c, t_c) = (1.0 / (3.0 * b), 8.0 * a / (27.0 * R * b));
    let side = |p, rho| SatSide { t, p, rho };
    if t == t_c {
        let p_c = a / (27.0 * b * b);
        return SatPair { bubble: side(p_c, rho_c), dew: side(p_c, rho_c) };
    }
    // (1 − bρ)²·(∂p/∂ρ)_T = RT − 2aρ(1 − bρ)²: positive at 0 and 1/b, negative at ρc below Tc.
    let stiffness = |rho: f64| R * t - 2.0 * a * rho * (1.0 - b * rho) * (1.0 - b * rho);
    let (spin_v, spin_l) = (bisect(|rho| -stiffness(rho), 0.0, rho_c), bisect(stiffness, rho_c, 1.0 / b));
    let densities = |p: f64| {
        let (dew, bubble) = (
            bisect(|rho| model.p(t, rho) - p, 0.0, spin_v),
            bisect(|rho| model.p(t, rho) - p, spin_l, (1.0 - 1e-12) / b),
        );
        (bubble, dew)
    };
    // g′ − g″ falls through zero as p rises from the liquid spinodal (or 0) to the vapour spinodal.
    let p_lo = model.p(t, spin_l).max(0.0);
    let p = bisect(
        |p| {
            let (l, v) = densities(p);
            model.g(t, v) - model.g(t, l)
        },
        p_lo,
        model.p(t, spin_v),
    );
    let (rho_l, rho_v) = densities(p);
    SatPair { bubble: side(p, rho_l), dew: side(p, rho_v) }
}

/// E18 / D6 (S-10): below Tc the out-of-tree family supplies its own `SaturationCurve` (a Maxwell construction, above)
/// and the core's phase rule does the rest with no core edit: the curve's top is the critical point, a state inside the
/// dome splits by the lever rule at the curve's pressure, either side is single-phase and stable, and below the
/// curve's range the generic VLE of the EOS decides (M7.2). Arbiter: the Maxwell conditions themselves, p′ = p″ and
/// g′ = g″, at the curve's densities through the core's own states (the bisection resolves them to a few ulp).
#[test]
fn new_family_subcritical_dt_goes_through_the_core_phase_rule() {
    let (a, b) = (0.1355, 3.2e-5);
    let model = VanDerWaals { a, b };
    let crit = model.critical();
    let source = Source::new("vanderWaals-1873", None, DataTerms::Published);
    let info = FluidInfo::new("vdW-Argon", 0.039_948, source, ModelKey::from_content(b"vdw a=0.1355 b=3.2e-5 maxwell"));
    let limits = Limits::new(50.0, 2000.0, 1e9).unwrap();
    let package = PureFluid::builder(info.unwrap(), model, limits).critical(crit).saturation(Maxwell { a, b }).build();
    let reg = Registry::empty().with_model(Arc::new(package)).unwrap();
    let fluid = reg.get("vdW-Argon").unwrap();
    let t = 0.8 * crit.t;
    let sat = Maxwell { a, b }.at_t(t).unwrap();
    let (p_sat, rho_l, rho_v) = (sat.bubble.p, sat.bubble.rho, sat.dew.rho);
    // The Maxwell conditions through the core: one pressure and one Gibbs energy on both sides.
    let imposed = |rho, phase| fluid.flash(dt(rho, t), &FlashOptions::new().with_phase(phase)).unwrap();
    let (liquid, vapour) = (imposed(rho_l, Phase::Liquid), imposed(rho_v, Phase::Gas));
    assert!((liquid.p() / p_sat - 1.0).abs() < 1e-12 && (vapour.p() / p_sat - 1.0).abs() < 1e-12, "{p_sat} Pa");
    let (g_l, g_v) = (liquid.g(Basis::Molar), vapour.g(Basis::Molar));
    assert!((g_l - g_v).abs() < 1e-12 * R * t, "g′ − g″ = {} J/mol", g_l - g_v);
    // Inside the dome: two phases at the curve's pressure, the quality by the lever rule on molar volume.
    let rho = 1.0 / (0.75 / rho_l + 0.25 / rho_v);
    let state = fluid.state(dt(rho, t)).unwrap();
    assert_eq!((state.phase(), state.path().strategy), (Phase::TwoPhase, Strategy::Superancillary));
    assert_eq!(state.p(), p_sat);
    assert!((state.quality().unwrap() - 0.25).abs() < 1e-12, "{:?}", state.quality());
    assert!((state.h(Basis::Molar) - (0.75 * liquid.h(Basis::Molar) + 0.25 * vapour.h(Basis::Molar))).abs() < 1e-9);
    // Either side of the curve: single phase and stable, labelled by the curve.
    let compressed = fluid.state(dt(1.01 * rho_l, t)).unwrap();
    assert_eq!((compressed.phase(), compressed.path().strategy), (Phase::Liquid, Strategy::Direct));
    assert_eq!(fluid.state(dt(0.99 * rho_v, t)).unwrap().phase(), Phase::Gas);
    // The curve's top is the model's critical point.
    let critical = fluid.state(dt(crit.rho, crit.t)).unwrap();
    assert_eq!(critical.phase(), Phase::CriticalPoint);
    assert_eq!(fluid.state(dt(0.5 * crit.rho, 1.01 * crit.t)).unwrap().phase(), Phase::SupercriticalGas);
    // Below the curve's range the generic VLE of the EOS decides (M7.2), the test's Maxwell construction its arbiter.
    let low = fluid.state(dt(rho, 0.4 * crit.t)).unwrap();
    assert_eq!((low.phase(), low.path().strategy), (Phase::TwoPhase, Strategy::Vle));
    assert!((low.p() / maxwell(a, b, 0.4 * crit.t).bubble.p - 1.0).abs() < 1e-12, "{}", low.p());
    // The batch driver and the strings reach the dome through the same flash.
    assert_eq!(props_si_in(&reg, "Q", "T", t, "Dmolar", rho, "vdW-Argon").unwrap(), state.quality().unwrap());
    let (x, y, outputs) = ([rho, 1.01 * rho_l], [t, t], [Prop::P, Prop::Q]);
    let request = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &outputs);
    let (mut out, mut status) = ([0.0; 4], [Status::Ok; 4]);
    batch::evaluate(fluid, &request, &mut out, &mut status).unwrap();
    assert_eq!(out[..2], [p_sat, state.quality().unwrap()]);
    // A single-phase state has no quality: `Undefined`, never a sentinel (ROT-013).
    assert_eq!(status, [Status::Ok, Status::Ok, Status::Ok, Status::Undefined]);
    assert!(out[3].is_nan());
}

/// E1 / E4: first partial derivatives, Z and Cp0 for a family the core has never seen; exact virials.
#[test]
fn derivative_outputs_reach_the_new_family() {
    let (model, crit) = vdw();
    let reg = Registry::empty().with_model(model).unwrap();
    let fluid = reg.get("vdw").unwrap();
    let (t, rho) = (2.0 * crit.t, 100.0);
    let s = fluid.state(dt(rho, t)).unwrap();
    let partial = |of, wrt, at| fluid.prop(&s, Prop::Partial(Partial { of, wrt, at })).unwrap();
    use DerivVar::{Dmolar, Hmass, Hmolar, P, Smolar, T};
    let cp = s.cp(Basis::Molar).unwrap();
    assert!((partial(Hmolar, T, P) / cp - 1.0).abs() < 1e-12);
    assert!((partial(Hmass, T, P) / s.cp(Basis::Mass).unwrap() - 1.0).abs() < 1e-12);
    assert!((partial(Smolar, T, P) * t / cp - 1.0).abs() < 1e-12);
    let dpdt = rho * R / (1.0 - 3.2e-5 * rho);
    assert!((partial(P, T, Dmolar) / dpdt - 1.0).abs() < 1e-13);
    assert!((fluid.prop(&s, Prop::Z).unwrap() - s.p() / (rho * R * t)).abs() < 1e-15);
    assert!((fluid.prop(&s, Prop::Cp0molar).unwrap() - 2.5 * R).abs() < 1e-12);
    // Exact virials against a low-density state: Z − 1 − Bρ − Cρ² = O((bρ)³).
    let v = fluid.model().helmholtz().unwrap().zero_density(t).unwrap();
    let low = fluid.state(dt(1.0, t)).unwrap();
    assert!((low.z() - 1.0 - v.b - v.c).abs() < 1e-13);
}

#[test]
fn batch_reports_every_cell_and_policies_agree() {
    let (model, crit) = vdw();
    let reg = Registry::empty().with_model(model).unwrap();
    let fluid = reg.get("vdw").unwrap();
    let t = 2.0 * crit.t;
    let (x, y) = ([100.0, f64::NAN, 200.0], [t, t, t]);
    let outputs = [Prop::P, Prop::Cpmolar, Prop::Viscosity];
    let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &outputs);
    let (mut out, mut status) = ([0.0; 9], [Status::Ok; 9]);
    let summary = batch::evaluate(fluid, &req, &mut out, &mut status).unwrap();
    let (ok, bad, none) = (Status::Ok, Status::InvalidInput, Status::NoModel);
    assert_eq!(status, [ok, ok, none, bad, bad, bad, ok, ok, none]);
    assert!(out[3..6].iter().all(|v| v.is_nan()) && summary.failed_cells == 5);
    let parallel = ExecPolicy::Parallel { chunk: NonZeroUsize::new(2).unwrap() };
    policy_equivalence(fluid, &req, parallel).unwrap();
}

#[cfg(not(target_family = "wasm"))] // wasip2 has no thread spawning; the corpus runs there without this test
#[test]
fn sixteen_threads_share_one_fluid_bitwise() {
    let (model, crit) = vdw();
    let reg = Registry::empty().with_model(model).unwrap();
    let grid: Vec<(f64, f64)> = (1..=64).map(|i| (f64::from(i) * 50.0, crit.t * (1.0 + f64::from(i) / 64.0))).collect();
    let p = |&(rho, t): &(f64, f64)| reg.get("vdw").unwrap().state(dt(rho, t)).unwrap().p().to_bits();
    let sequential: Vec<u64> = grid.iter().map(p).collect();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..16).map(|_| s.spawn(|| grid.iter().map(p).collect::<Vec<u64>>())).collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), sequential);
        }
    });
}

#[test]
fn compat_strings_reach_the_new_family() {
    let (model, crit) = vdw();
    let reg = Registry::empty().with_model(model).unwrap();
    let (t, rho) = (2.0 * crit.t, 100.0);
    let p = reg.get("vdW-Argon").unwrap().state(dt(rho, t)).unwrap().p();
    assert_eq!(props_si_in(&reg, "P", "T", t, "Dmolar", rho, "vdW-Argon").unwrap(), p);
    assert_eq!(props_si_in(&reg, "P", "Dmolar", rho, "T", t, "HEOS::vdw").unwrap(), p);
    let refused = props_si_in(&reg, "P", "H", 1e3, "T", t, "vdw");
    assert_eq!(refused, Err(CompatError::Core(Error::Unsupported { pair: Pair::HT })));
}

#[test]
fn reference_states_work_for_any_family() {
    let (model, crit) = vdw();
    let reg = Registry::empty().with_model(model).unwrap();
    let fluid = reg.get("vdw").unwrap();
    let (t, rho) = (2.0 * crit.t, 100.0);
    let anchor = dt(rho, t);
    let at_zero =
        ReferenceState::Custom { at: anchor, h: Enthalpy::molar(0.0).unwrap(), s: Entropy::molar(0.0).unwrap() };
    let shifted = fluid.with_reference(at_zero).unwrap();
    assert!(shifted.state(anchor).unwrap().h(Basis::Molar).abs() < 1e-9);
    gauge_invariance(fluid, shifted.gauge(), &[anchor, dt(300.0, 1.5 * crit.t)]).unwrap();
    // IIR anchors on saturated liquid (QT): refused until the family declares QT, never a panic.
    assert_eq!(fluid.with_reference(ReferenceState::Iir).unwrap_err(), Error::Unsupported { pair: Pair::QT });
    // E10: the registry form reaches strings (and so C and JS) under every alias.
    let gauged = reg.with_reference("VDW-ARGON", at_zero).unwrap();
    assert!(props_si_in(&gauged, "Hmolar", "T", t, "Dmolar", rho, "vdw").unwrap().abs() < 1e-9);
    assert!(props_si_in(&reg, "Hmolar", "T", t, "Dmolar", rho, "vdw").unwrap().abs() > 1.0);
}
