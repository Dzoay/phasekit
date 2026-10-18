//! GERG-2008 (Kunz and Wagner 2012; PLAN.md M13.5), the multi-fluid model of natural gases over 21 components, each
//! with GERG's own equation ([`pure`]), its 210 binaries' reducing parameters and the departure functions of 15 of
//! them. R = 8.314472 J/(mol·K), part of the model's definition (user decision MX3). Its names carry the prefix
//! `GERG2008::` (user decision MX6): `GERG2008::Methane` for a pure component, `GERG2008::Methane[0.9]&Ethane[0.1]` for
//! a mixture; components resolve through the registry's names and aliases. The data are NIST's AGA8 code's (user
//! decision MX7), immutable statics (ROT-204).

mod pure;
#[rustfmt::skip] // a transcription, one term a line (`gerg_2008_is_aga8s` checks it number for number)
mod tables;

use std::sync::Arc;

use std::any::Any;

use phasekit_core::{
    Constants, CriticalOrigin, CriticalPoint, DataTerms, Error, Fluid, FluidInfo, Limits, LoadError, ModelKey,
    PureFluid, Reducing, Registry, SatAccuracy, SatPair, SaturationCurve, Source,
};

pub(crate) use pure::Pure;
pub(crate) use tables::{BINARIES, COMPONENTS, DEPARTURES};

use crate::data::{DepartureData, Term};

/// GERG-2008's molar gas constant, J/(mol·K) (Kunz and Wagner 2012; user decision MX3).
pub const R_GERG: f64 = 8.314_472;

/// The prefix of GERG-2008's names (user decision MX6).
pub const PREFIX: &str = "GERG2008::";

/// GERG-2008's range in temperature (K) and pressure (Pa), its extended range of validity (Kunz and Wagner 2012 §1):
/// every GERG fluid's `Limits`, each with its own triple point (ROT-205).
const RANGE: (f64, f64, f64) = (60.0, 700.0, 70e6);

/// One component (AGA8's `MMiGERG`, `Dc`, `Tc`, `noik`/`doik`/`toik`/`coik`, `n0i` and `th0i`).
#[derive(Debug)]
pub(crate) struct Component {
    /// AGA8's name for it.
    pub name: &'static str,
    /// Its CAS number: phasekit's key into the registry, not AGA8's.
    pub cas: &'static str,
    /// M, g/mol.
    pub molar_mass: f64,
    /// ρ_c, mol/l.
    pub rho_c: f64,
    /// T_c, K.
    pub t_c: f64,
    /// n δ^d τ^t exp(−δ^l): l is AGA8's c, the polynomial terms (l = 0) first.
    pub residual: &'static [Term],
    /// n₁ ... n₇ as AGA8 writes them: n₁ and n₂ its T-form constants, rounded; n₃ c_p⁰'s constant (α⁰'s is n₃ − 1).
    pub n0: [f64; 7],
    /// θ₄ ... θ₇, K: sinh at 4 and 6, cosh at 5 and 7; 0 where the term is absent.
    pub theta: [f64; 4],
}

/// One binary, i < j in AGA8's order (from 0): its reducing parameters, F and its departure function's model number.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Binary {
    pub i: usize,
    pub j: usize,
    pub beta_v: f64,
    pub gamma_v: f64,
    pub beta_t: f64,
    pub gamma_t: f64,
    pub f: f64,
    pub model: Option<usize>,
}

/// The departure function of AGA8's model number `model`.
pub(crate) fn departure(model: usize) -> Option<&'static DepartureData> {
    DEPARTURES.iter().find(|(m, _)| *m == model).map(|(_, f)| f)
}

/// The binary of components `a` and `b` (AGA8's order, from 0, either way round) and whether `(a, b)` reverses it.
pub(crate) fn binary(a: usize, b: usize) -> Option<(&'static Binary, bool)> {
    let (i, j, swapped) = match a.cmp(&b) {
        std::cmp::Ordering::Less => (a, b, false),
        std::cmp::Ordering::Greater => (b, a, true),
        std::cmp::Ordering::Equal => return None,
    };
    BINARIES.iter().find(|p| p.i == i && p.j == j).map(|p| (p, swapped))
}

/// The GERG component `name` resolves to in `registry` (its name, an alias or its CAS number), with that fluid.
pub(crate) fn component(registry: &Registry, name: &str) -> Result<(usize, Fluid), Error> {
    let fluid = registry.resolve(name)?;
    let cas = crate::multifluid::cas(fluid.info().name());
    COMPONENTS
        .iter()
        .position(|c| Some(c.cas) == cas)
        .map(|i| (i, fluid))
        .ok_or_else(|| Error::Load(LoadError::UnknownName(format!("{PREFIX}{name}").into())))
}

/// GERG component `i`'s pure fluid, named after `fluid`, the registry's of the same substance, which also gives its
/// aliases, triple point and density bound.
pub(crate) fn pure_fluid(i: usize, fluid: &Fluid) -> Result<PureFluid, Error> {
    let c = COMPONENTS.get(i).ok_or(Error::InvalidInput { quantity: "GERG-2008 component", value: i as f64 })?;
    let eos = Pure::new(c, fluid.clone());
    let source = fluid.info();
    let name = format!("{PREFIX}{}", source.name());
    let aliases: Vec<String> = source.aliases().map(|alias| format!("{PREFIX}{alias}")).collect();
    let aliases: Vec<&str> = aliases.iter().map(String::as_str).collect();
    let t_triple = fluid.model().limits().and_then(|l| l.t_triple());
    let key = format!("{name}|{t_triple:?}|{:?}", source.key());
    let (t_c, rho_c) = (c.t_c, 1e3 * c.rho_c);
    let p_c = {
        use phasekit_core::{HelmholtzModel, Order};
        let a01 = eos.residual(t_c, rho_c, Order::One).get(0, 1).unwrap_or(f64::NAN);
        rho_c * R_GERG * t_c * (1.0 + a01)
    };
    let critical = CriticalPoint { t: t_c, p: p_c, rho: rho_c, origin: CriticalOrigin::Published };
    let info = FluidInfo::new(
        &name,
        1e-3 * c.molar_mass,
        Source::new("Kunz-JCED-2012", Some("10.1021/je300655b"), DataTerms::Published),
        ModelKey::from_content(key.as_bytes()),
    )?
    .with_aliases(&aliases)
    .with_published_critical(critical)
    .with_constants(Constants::new(Some(Reducing { t: t_c, rho: rho_c, p: None }), None, None));
    let (t_min, t_max, p_max) = RANGE;
    let limits = Limits::new(t_min, t_max, p_max)?;
    let limits = t_triple.map_or(limits, |t| limits.with_t_triple(t));
    let builder = PureFluid::builder(info, eos, limits).critical(critical);
    Ok(match reference_curve(fluid) {
        Some(_) => builder.saturation(Seeds { reference: fluid.clone(), t_c }).build(),
        None => builder.build(),
    })
}

/// The saturation curve of `fluid`, the registry's model of the same substance, if it has one.
fn reference_curve(fluid: &Fluid) -> Option<&dyn SaturationCurve> {
    let package = (fluid.model() as &dyn Any).downcast_ref::<PureFluid>()?;
    package.saturation().ok().flatten()
}

/// A GERG pure fluid's saturation (PLAN.md M13.5: core's VLE serves it): the registry's curve for the same substance
/// as a starting point only (`SatAccuracy::Guess`), which core's VLE polishes on GERG's own equation, over that curve's
/// range below GERG's critical temperature.
#[derive(Debug)]
struct Seeds {
    reference: Fluid,
    t_c: f64,
}

impl Seeds {
    fn curve(&self) -> Result<&dyn SaturationCurve, Error> {
        reference_curve(&self.reference).ok_or(Error::Unsupported { pair: phasekit_core::Pair::QT })
    }
}

impl SaturationCurve for Seeds {
    fn accuracy(&self) -> SatAccuracy {
        SatAccuracy::Guess
    }

    fn t_range(&self) -> (f64, f64) {
        let (lo, hi) = self.curve().map_or((f64::NAN, f64::NAN), |c| c.t_range());
        (lo, hi.min(self.t_c))
    }

    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        self.curve()?.at_t(t)
    }

    fn at_p(&self, p: f64) -> Result<SatPair, Error> {
        self.curve()?.at_p(p)
    }
}

/// `registry` with GERG-2008's pure fluids, `GERG2008::` and the registry's name of each of its 21 components (and its
/// aliases so prefixed); a component the registry lacks is left out.
pub fn with_gerg2008(registry: &Registry) -> Result<Registry, Error> {
    let mut out = registry.clone();
    for c in &COMPONENTS {
        let Ok((i, fluid)) = component(registry, c.cas) else { continue };
        out = out.with_model(Arc::new(pure_fluid(i, &fluid)?))?;
    }
    Ok(out)
}

/// GERG-2008's transcription keyed as AGA8's `SetupGERG` assigns it, its array name and its indices from 1, for
/// `cargo xtask`'s `gerg_2008_is_aga8s`: per component `MMiGERG`, `Dc`, `Tc`, `kpol`, `kexp`, `noik`, `doik`, `toik`,
/// `coik`, `n0i` and `th0i`; per binary (i < j) `bvij`, `gvij`, `btij`, `gtij`, `fij` and `mNumb` (−1 for none); per
/// departure function `kpolij`, `kexpij`, `nijk`, `dijk`, `tijk`, `cijk` (η), `eijk` (ε), `bijk` (β) and `gijk` (γ),
/// the last four 0 for its polynomial terms.
pub(crate) fn aga8_entries() -> Vec<(&'static str, Vec<usize>, f64)> {
    let mut out = Vec::new();
    // AGA8 numbers components, terms and binaries from 1.
    let one = |k: usize| k.saturating_add(1);
    for (i, c) in COMPONENTS.iter().enumerate().map(|(i, c)| (one(i), c)) {
        let polynomial = c.residual.iter().take_while(|t| t.l == 0.0).count();
        for (name, value) in [("MMiGERG", c.molar_mass), ("Dc", c.rho_c), ("Tc", c.t_c)] {
            out.push((name, vec![i], value));
        }
        out.push(("kpol", vec![i], polynomial as f64));
        out.push(("kexp", vec![i], c.residual.len().saturating_sub(polynomial) as f64));
        for (k, t) in c.residual.iter().enumerate().map(|(k, t)| (one(k), t)) {
            for (name, value) in [("noik", t.n), ("doik", t.d), ("toik", t.t), ("coik", t.l)] {
                out.push((name, vec![i, k], value));
            }
        }
        out.extend(c.n0.iter().enumerate().map(|(j, n)| ("n0i", vec![i, one(j)], *n)));
        out.extend(c.theta.iter().enumerate().map(|(j, theta)| ("th0i", vec![i, j.saturating_add(4)], *theta)));
    }
    for b in &BINARIES {
        let (i, j) = (one(b.i), one(b.j));
        let model = b.model.map_or(-1.0, |m| m as f64);
        for (name, value) in [
            ("bvij", b.beta_v),
            ("gvij", b.gamma_v),
            ("btij", b.beta_t),
            ("gtij", b.gamma_t),
            ("fij", b.f),
            ("mNumb", model),
        ] {
            out.push((name, vec![i, j], value));
        }
    }
    for (m, f) in &DEPARTURES {
        out.push(("kpolij", vec![*m], f.power as f64));
        out.push(("kexpij", vec![*m], f.terms.len().saturating_sub(f.power) as f64));
        for (k, t) in f.terms.iter().enumerate().map(|(k, t)| (one(k), t)) {
            let arrays = [
                ("nijk", t.n),
                ("dijk", t.d),
                ("tijk", t.t),
                ("cijk", t.eta),
                ("eijk", t.epsilon),
                ("bijk", t.beta),
                ("gijk", t.gamma),
            ];
            out.extend(arrays.map(|(name, v)| (name, vec![*m, k], v)));
        }
    }
    out
}

/// Each component's ideal-gas n₁ and n₂ as AGA8 writes them, computed exactly ([`pure::aga8_constants`]).
pub(crate) fn aga8_n1_n2() -> Vec<[f64; 2]> {
    COMPONENTS.iter().map(pure::aga8_constants).collect()
}
