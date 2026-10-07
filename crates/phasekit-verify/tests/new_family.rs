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
    FluidInfo, HelmholtzModel, Input, Limits, ModelKey, Order, Pair, Partial, Phase, Prop, PureFluid, ReferenceState,
    Registry, Source, Strategy, Temperature, ThermoModel, Virials, math,
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
    let pt = Input::new(Pair::PT, 1e5, t, Basis::Molar).unwrap();
    assert_eq!(fluid.flash(pt, &FlashOptions::default()).unwrap_err(), Error::Unsupported { pair: Pair::PT });
}

/// S-10 / E18: below Tc a family with no saturation curve needs the generic VLE (M6). Until then the core
/// says `Unsupported`, never a load error or a guess; an imposed single phase is honoured.
#[test]
fn subcritical_without_a_curve_is_unsupported_until_m6() {
    let (model, crit) = vdw();
    let reg = Registry::empty().with_model(model).unwrap();
    let fluid = reg.get("vdw").unwrap();
    let input = dt(100.0, 0.8 * crit.t);
    assert_eq!(fluid.state(input).unwrap_err(), Error::Unsupported { pair: Pair::DT });
    let gas = fluid.flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap();
    assert_eq!(gas.phase(), Phase::Gas);
    let two_phase = FlashOptions::new().with_phase(Phase::TwoPhase);
    assert_eq!(fluid.flash(input, &two_phase).unwrap_err(), Error::Unsupported { pair: Pair::DT });
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
    let refused = props_si_in(&reg, "P", "P", 1e5, "T", t, "vdw");
    assert_eq!(refused, Err(CompatError::Core(Error::Unsupported { pair: Pair::PT })));
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
