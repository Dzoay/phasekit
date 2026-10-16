//! The mixture seams in core (PLAN.md M13.1, user decision MX2): a two-component toy family defined outside the core
//! builds splits whose phases differ in molar mass, receives a mass quality as one from Rust, compat strings and the
//! batch driver, is built from a composed name by a registry resolver, and solves its phases' densities through the
//! public density solve. `phasekit-mix` (M13) takes these exact paths.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::sync::Arc;

use phasekit_compat::{batch_si, props_si_in};
use phasekit_core::batch::{self, BatchRequest, Status};
use phasekit_core::density::{self, Branch};
use phasekit_core::{
    Basis, Bundle, Capabilities, DataTerms, Derivs, Error, FlashOptions, Fluid, FluidInfo, HelmholtzModel, Input,
    ModelKey, NativeInput, Order, Pair, Phase, Prop, Quality, Registry, Resolver, RootPolicy, Source, State,
    Temperature, ThermoModel, math,
};

const R: f64 = 8.314_462_618;

/// The molar masses of the toy's two components, kg/mol, and the width of its dome in mole fraction.
const M_A: f64 = 0.03;
const M_B: f64 = 0.05;
const D: f64 = 0.4;

/// A binary at a fixed feed z (A's mole fraction) with a toy dome: its split at molar quality q has the liquid
/// x = z − qD and the vapour y = z + (1 − q)D (A's fractions), so the mass balance (1 − q)x + qy = z holds by
/// construction; each phase is an ideal-gas-like bundle at its own density and molar mass. Declares QT only.
#[derive(Debug)]
struct ToyMix {
    z: f64,
    info: FluidInfo,
}

impl ToyMix {
    fn new(name: &str, z: f64) -> ToyMix {
        let source = Source::new("toy-mixture", None, DataTerms::Published);
        let key = ModelKey::from_content(format!("toy mixture z={z}").as_bytes());
        ToyMix { z, info: FluidInfo::new(name, Self::molar_mass(z), source, key).unwrap() }
    }

    /// The molar mass of a phase with A's mole fraction `x`.
    fn molar_mass(x: f64) -> f64 {
        x * M_A + (1.0 - x) * M_B
    }

    fn phase(&self, t: f64, rho: f64, x: f64, phase: Phase) -> Result<State, Error> {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        State::from_total(self.info.key(), t, rho, R, Self::molar_mass(x), phase, &b)
    }

    fn split(&self, t: f64, q: f64) -> Result<State, Error> {
        let (x, y) = (self.z - q * D, self.z + (1.0 - q) * D);
        let liquid = self.phase(t, 20_000.0, x, Phase::Liquid)?;
        let vapour = self.phase(t, 40.0, y, Phase::Gas)?;
        State::from_split(liquid, vapour, Quality::new(q)?, t, vapour.p())
    }

    /// The vapour's share of the mass at molar quality `q`: qM(y)/M(z), increasing in q.
    fn mass_share(&self, q: f64) -> f64 {
        q * Self::molar_mass(self.z + (1.0 - q) * D) / Self::molar_mass(self.z)
    }

    /// The molar quality whose mass quality is `w`, by bisection.
    fn molar_quality(&self, w: f64) -> f64 {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..200 {
            let mid = 0.5 * (lo + hi);
            if self.mass_share(mid) < w { lo = mid } else { hi = mid }
        }
        0.5 * (lo + hi)
    }
}

impl ThermoModel for ToyMix {
    fn info(&self) -> &FluidInfo {
        &self.info
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::none().with(Pair::QT)
    }
    fn flash(&self, input: NativeInput, _opts: &FlashOptions) -> Result<State, Error> {
        match (input.pair(), input.values()) {
            (Pair::QT, (q, t)) => {
                let q = if input.quality_basis() == Basis::Mass { self.molar_quality(q) } else { q };
                self.split(t, q)
            }
            (pair, _) => Err(Error::Unsupported { pair }),
        }
    }
}

fn qt(q: f64, t: f64) -> Input {
    Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap())
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    ((a - b) / b).abs() <= tol
}

/// A split's mass quality, mass-basis values and `Prop::Qmass` weigh its phases by their own molar masses.
#[test]
fn a_mixture_split_weighs_its_phases_by_mass() {
    let toy = ToyMix::new("ToyMix", 0.5);
    let share = toy.mass_share(0.4);
    let mix = Fluid::new(Arc::new(toy));
    let state = mix.flash(qt(0.4, 300.0), &FlashOptions::new());
    assert!(state.is_ok(), "{state:?}");
    let state = state.unwrap();
    let m_z = ToyMix::molar_mass(0.5);
    assert_eq!(state.quality(), Some(0.4));
    assert!(state.mass_quality().is_some_and(|w| close(w, share, 1e-15)), "{:?}", state.mass_quality());
    assert!(mix.prop(&state, Prop::Qmass).is_ok_and(|w| close(w, share, 1e-15)));
    assert!(close(state.h(Basis::Mass), state.h(Basis::Molar) / m_z, 1e-15));
    assert!(close(state.rho(Basis::Mass), state.rho(Basis::Molar) * m_z, 1e-15));
}

/// A mass quality reaches the family as one, from a Rust input, a compat `Qmass` key and the batch driver; `Q` stays
/// molar beside a mass-basis key.
#[test]
fn a_mass_quality_reaches_the_family_from_rust_compat_and_batch() {
    let toy = ToyMix::new("ToyMix", 0.5);
    let (w, q) = (0.4, toy.molar_quality(0.4));
    let reg = Registry::embedded().unwrap().with_model(Arc::new(toy)).unwrap();
    let mix = reg.get("toymix").unwrap();
    let state = mix.flash(qt(w, 300.0).with_quality_basis(Basis::Mass), &FlashOptions::new());
    assert!(state.as_ref().is_ok_and(|s| s.quality() == Some(q)), "{state:?} against q = {q}");
    assert_eq!(props_si_in(&reg, "Q", "Qmass", w, "T", 300.0, "ToyMix").ok(), Some(q));
    assert_eq!(
        props_si_in(&reg, "Qmass", "T", 300.0, "Q", q, "ToyMix").ok(),
        state.ok().and_then(|s| s.mass_quality())
    );
    let (x, y, outputs) = ([w], [300.0], [Prop::Q]);
    let req = BatchRequest::new(Pair::QT, Basis::Molar, &x, &y, &outputs).with_quality_basis(Basis::Mass);
    let (mut out, mut status) = ([0.0], [Status::Ok]);
    batch::evaluate(mix, &req, &mut out, &mut status).unwrap();
    assert_eq!((out, status), ([q], [Status::Ok]));
    batch_si(mix, &["Q"], ("Qmass", &[w]), ("T", &[300.0]), &mut out, &mut status).unwrap();
    assert_eq!((out, status), ([q], [Status::Ok]));
}

/// Reads `ToyA[z]&ToyB[w]`: a `ToyMix` at A's fraction z, refused unless z + w = 1.
#[derive(Debug)]
struct ToyResolver;

impl Resolver for ToyResolver {
    fn resolve(&self, name: &str, _registry: &Registry) -> Option<Result<Arc<dyn ThermoModel>, Error>> {
        let (z, w) = name.strip_prefix("ToyA[")?.strip_suffix(']')?.split_once("]&ToyB[")?;
        let (z, w) = (z.parse::<f64>().ok()?, w.parse::<f64>().ok()?);
        let refused = Error::InvalidInput { quantity: "mole fractions' sum", value: z + w };
        Some(if z + w == 1.0 { Ok(Arc::new(ToyMix::new(name, z)) as Arc<dyn ThermoModel>) } else { Err(refused) })
    }
}

/// A composed name reaches the family through a resolver layer; held names resolve as `get` gives them.
#[test]
fn a_composed_name_resolves_to_the_family_s_model() {
    let reg = Registry::embedded().unwrap().with_resolver(Arc::new(ToyResolver));
    let mix = reg.resolve("ToyA[0.5]&ToyB[0.5]");
    assert!(mix.as_ref().is_ok_and(|f| f.info().name() == "ToyA[0.5]&ToyB[0.5]"), "{mix:?}");
    let state = mix.unwrap().flash(qt(0.4, 300.0), &FlashOptions::new());
    let direct = ToyMix::new("direct", 0.5).split(300.0, 0.4).map(|s| s.h(Basis::Mass));
    assert_eq!(state.map(|s| s.h(Basis::Mass)), direct);
    let refused = Error::InvalidInput { quantity: "mole fractions' sum", value: 0.5 + 0.6 };
    assert_eq!(reg.resolve("ToyA[0.5]&ToyB[0.6]").err(), Some(refused));
    assert!(reg.resolve("Water").is_ok() && reg.get("ToyA[0.5]&ToyB[0.5]").is_err());
}

/// van der Waals in (T, ρ), as `tests/new_family.rs` writes it, with a monatomic ideal gas.
#[derive(Debug)]
struct VanDerWaals {
    a: f64,
    b: f64,
}

impl VanDerWaals {
    const ARGON: VanDerWaals = VanDerWaals { a: 0.1355, b: 3.2e-5 };

    fn t_c(&self) -> f64 {
        8.0 * self.a / (27.0 * R * self.b)
    }
    fn p_c(&self) -> f64 {
        self.a / (27.0 * self.b * self.b)
    }
    fn pressure(&self, t: f64, rho: f64) -> f64 {
        rho * R * t / (1.0 - self.b * rho) - self.a * rho * rho
    }
    fn dp_drho(&self, t: f64, rho: f64) -> f64 {
        R * t / ((1.0 - self.b * rho) * (1.0 - self.b * rho)) - 2.0 * self.a * rho
    }
    /// The molar Gibbs energy up to a function of T alone: a + p/ρ with a/RT = ln ρ + α^r.
    fn gibbs(&self, t: f64, rho: f64) -> f64 {
        R * t * (math::ln(rho) - math::ln(1.0 - self.b * rho)) - self.a * rho + self.pressure(t, rho) / rho
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
        Derivs::from_fn(order, |i, j| match (i, j) {
            (0, 0) => math::ln(rho) - 1.5 * math::ln(t),
            (1, 0) => 1.5,
            (2, 0) => -1.5,
            (0, j) => Derivs::IDEAL_DELTA.get(0, j).unwrap_or(0.0),
            _ => 0.0,
        })
    }
    fn rho_max(&self, _t: f64) -> f64 {
        0.999 / self.b
    }
}

/// The public density solve (ROT-181's mechanism for a family): below T_c, between the spinodal pressures, it finds
/// the liquid and the vapour root, each reproducing p on a stable branch, never the unstable middle root; it selects
/// among them under each `RootPolicy`; above T_c it finds the one root.
#[test]
fn the_public_density_solve_finds_vdw_s_stable_roots() {
    let vdw = VanDerWaals::ARGON;
    let (t, p) = (0.9 * vdw.t_c(), 0.65 * vdw.p_c());
    let roots = density::roots(&vdw, t, p);
    let [vapour, liquid] = *roots.as_slice() else { panic!("{roots:?}") };
    for rho in [vapour, liquid] {
        assert!(close(vdw.pressure(t, rho), p, 1e-12) && vdw.dp_drho(t, rho) > 0.0, "{rho}");
    }
    assert!(vapour < 1.0 / (3.0 * vdw.b) && 1.0 / (3.0 * vdw.b) < liquid);
    assert_eq!(density::solve(&vdw, t, p, Branch::Liquid, None), Ok(liquid));
    assert_eq!(density::solve(&vdw, t, p, Branch::Vapour, Some(1.0 / (3.0 * vdw.b))), Ok(vapour), "unstable seed");
    assert_eq!(density::select(&vdw, t, p, RootPolicy::Strict), Err(Error::Ambiguous { roots }));
    assert_eq!(density::select(&vdw, t, p, RootPolicy::Nearest(0.9 * liquid)), Ok(liquid));
    let stable = if vdw.gibbs(t, liquid) < vdw.gibbs(t, vapour) { liquid } else { vapour };
    assert_eq!(density::select(&vdw, t, p, RootPolicy::Stable), Ok(stable));
    // Either side of the saturation pressure (0.647 p_c at 0.9 T_c) the other root is stable: the vapour at 0.55 p_c,
    // the liquid at 0.70 p_c, each the one of lower Gibbs energy.
    for (p, liquid_stable) in [(0.55 * vdw.p_c(), false), (0.70 * vdw.p_c(), true)] {
        let [vapour, liquid] = *density::roots(&vdw, t, p).as_slice() else { panic!("{p}") };
        assert_eq!(vdw.gibbs(t, liquid) < vdw.gibbs(t, vapour), liquid_stable, "{p}");
        let want = if liquid_stable { liquid } else { vapour };
        assert_eq!(density::select(&vdw, t, p, RootPolicy::Stable), Ok(want), "{p}");
    }
    let (t, p) = (1.5 * vdw.t_c(), 2.0 * vdw.p_c());
    let [one] = *density::roots(&vdw, t, p).as_slice() else { panic!() };
    assert!(close(vdw.pressure(t, one), p, 1e-12));
    assert_eq!(density::select(&vdw, t, p, RootPolicy::Strict), Ok(one));
}

/// The acceptance gate is the core flash's: a state reproduces its pressure, a perturbed one does not.
#[test]
fn the_acceptance_gate_reproduces_the_pressure() {
    let vdw = VanDerWaals::ARGON;
    let (t, rho) = (300.0, 1_000.0);
    let b = (vdw.ideal(t, rho, Order::Two) + vdw.residual(t, rho, Order::Two)).bundle().unwrap();
    let p = vdw.pressure(t, rho);
    assert!(density::reproduces(R, t, rho, p, &b));
    assert!(!density::reproduces(R, t, rho, p * (1.0 + 1e-9), &b));
}
