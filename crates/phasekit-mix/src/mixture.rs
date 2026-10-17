//! A mixture at a fixed composition z (D4; PLAN.md M13.4): a `HelmholtzModel` with its own `ThermoModel`.
//!
//! - R is CoolProp's `R_CODATA` for every mixture of two or more components (user decision MX3); a composition that
//!   trims to one component is that pure fluid itself ([`mixture`]).
//! - α^r is the multi-fluid residual at z (M13.3).
//! - α⁰ keeps core's contract: its δ-part is ln δ exactly, so p, the density solve and the acceptance gate agree. Each
//!   component's τ-part enters weighted by z_i R_i/R, so the ideal gas of each component is its own (MX3, as v8.0.0):
//!   α⁰ = ln ρ + Σ z_i (R_i/R) [α⁰_oi(T, ρ) − ln ρ + ln z_i], each α⁰_oi in the component's own variables (ROT-062,
//!   ROT-103). The components' reference-state offsets are in their α⁰, so the mixture's native gauge is their
//!   z-weighted gauge.
//! - Its limits are the components' intersection (ROT-197).
//!
//! A PT flash without a phase hint runs the tangent-plane stability test on the feed's stable root and splits where it
//! fails (M13.8): a stable feed is labelled from the critical point, a split is two phases of their own compositions,
//! and a liquid-liquid split is refused (MX4). With a hint, DT is the state at (T, ρ) and PT the density on the hinted
//! branch through core's public density solve (M13.1). QT and PQ are two coexisting phases (M13.9, M13.10), and every
//! other pair an outer solve on them and on the feed's homogeneous states (`pairs`, M13.11).

use std::sync::{Arc, OnceLock};

use phasekit_core::density::Branch;
use phasekit_core::{
    Basis, Bundle, Capabilities, Constant, CriticalOrigin, CriticalPoint, DataSet, DataTerms, Derivs, DomainPolicy,
    Error, FlashOptions, Fluid, FluidInfo, HelmholtzModel, Limits, ModelKey, NativeInput, Order, Pair, Phase, Prop,
    Quality, Reducing, Registry, Source, State, ThermoModel, Virials, math,
};

use crate::critical::{CriticalState, Criticality, critical_points};
use crate::equilibrium::{self, Stability};
use crate::illinois::{End, illinois};
use crate::saturation::{self, Coexisting, Given, Saturation, Side};
use crate::{Composition, MixtureModel, MultiFluid, Var, gerg};

mod pairs;

/// Secant steps of a mass quality's molar vapour fraction.
const MASS_STEPS: usize = 30;

/// The isochoric split's search: widenings of its bracket, regula falsi steps, and the bracket's width in ln p at which
/// it ends.
const ISOCHORIC_STEPS: usize = 40;
const ISOCHORIC_ITERATIONS: usize = 100;
const ISOCHORIC_WIDTH: f64 = 1e-14;

/// The molar gas constant of every mixture of two or more components, J/(mol·K): CODATA 2018's, CoolProp 8.0.0's
/// `R_U_CODATA` (user decision MX3).
pub const R_CODATA: f64 = 8.314_462_618_153_24;

/// Two fractions within this of each other's sum of 1 are a composition (a ternary written to 6 digits sums to 1 within
/// 1e-6, so it is refused, not normalised: no input is read leniently).
const SUM_TOLERANCE: f64 = 1e-10;

/// A multi-fluid mixture at a fixed composition.
#[derive(Debug)]
pub struct Mixture {
    fluid: MultiFluid,
    z: Box<[f64]>,
    /// Each component's R_i/R.
    r_ratio: Box<[f64]>,
    info: FluidInfo,
    limits: Limits,
    /// Every critical point in the model's range, found on first use (ROT-187).
    critical: OnceLock<Vec<CriticalState>>,
}

/// Refuses a composition that is not one: a fraction not finite or outside [0, 1], or fractions not summing to 1
/// (ROT-193).
fn validated(z: &[f64]) -> Result<(), Error> {
    if let Some(bad) = z.iter().find(|x| !(x.is_finite() && (0.0..=1.0).contains(*x))) {
        return Err(Error::InvalidInput { quantity: "mole fraction", value: *bad });
    }
    let sum: f64 = z.iter().sum();
    if (sum - 1.0).abs() > SUM_TOLERANCE {
        return Err(Error::InvalidInput { quantity: "sum of the mole fractions", value: sum });
    }
    Ok(())
}

impl Mixture {
    /// `fluid` at mole fractions `z`, one per component, each > 0 ([`mixture`] trims zeros), summing to 1.
    pub fn new(fluid: MultiFluid, z: &[f64]) -> Result<Mixture, Error> {
        validated(z)?;
        if z.len() != fluid.len() || z.contains(&0.0) {
            return Err(Error::InvalidInput { quantity: "number of non-zero mole fractions", value: z.len() as f64 });
        }
        let fluids: Vec<&Fluid> = fluid.fluids().collect();
        let model = |f: &Fluid| f.model().helmholtz().map(|m| m.gas_constant()).unwrap_or(f64::NAN);
        let r_ratio = fluids.iter().map(|f| model(f) / fluid.gas_constant()).collect();
        let prefix = fluid.prefix();
        let label = |f: &Fluid| f.info().name().strip_prefix(prefix).unwrap_or(f.info().name()).to_string();
        let parts: Vec<String> = fluids.iter().zip(z).map(|(f, x)| format!("{}[{x}]", label(f))).collect();
        let name = format!("{prefix}{}", parts.join("&"));
        let molar_mass = fluids.iter().zip(z).map(|(f, x)| x * f.info().molar_mass()).sum();
        let mut content = name.clone().into_bytes();
        for f in &fluids {
            content.extend(format!("{:?}", f.info().key()).bytes());
        }
        let source = Source::new("Kunz-JCED-2012", Some("10.1021/je300655b"), DataTerms::Published);
        let info = FluidInfo::new(&name, molar_mass, source, ModelKey::from_content(&content))?;
        let limits = intersection(&fluids)?;
        Ok(Mixture { fluid, z: z.into(), r_ratio, info, limits, critical: OnceLock::new() })
    }

    /// The mole fractions, one per component.
    pub fn composition(&self) -> &[f64] {
        &self.z
    }

    /// The multi-fluid model.
    pub fn model(&self) -> &MultiFluid {
        &self.fluid
    }

    /// Every critical point of this mixture in its range, in increasing density (PLAN.md M13.7): found once, on first
    /// use, and kept (ROT-187). Points at negative pressure (on a liquid branch of the stability limit) are among them.
    pub fn critical_points(&self) -> &[CriticalState] {
        self.critical.get_or_init(|| self.critical_points_weighted(&self.r_ratio))
    }

    /// The critical points with the ideal part's composition terms weighted by `w` in place of R_i/R.
    pub(crate) fn critical_points_weighted(&self, w: &[f64]) -> Vec<CriticalState> {
        let (t_r, _) = self.fluid.reducing_state(&self.z);
        let range = (self.limits.t_min(), self.limits.t_max());
        critical_points(&self.fluid, &self.z, w, range, self.rho_max(t_r)).unwrap_or_default()
    }

    /// Heidemann and Khalil's conditions at (T, ρ): det(D Q D), D = diag(√z_i), and the cubic form along Q's
    /// D-normalised null vector, both 0 at a critical point (PLAN.md M13.7).
    pub fn criticality(&self, t: f64, rho: f64) -> Result<Criticality, Error> {
        crate::critical::conditions_at(&self.fluid, &self.z, &self.r_ratio, t, rho)
    }

    /// The residual's composition derivatives at (T, ρ) and this mixture's composition, to `order` (PLAN.md M13.6).
    pub fn composition_at(&self, t: f64, rho: f64, order: Order) -> Result<Composition, Error> {
        self.fluid.composition(t, rho, &self.z, order)
    }

    /// The chemical potentials μ_i = RT(∂Ψ/∂n_i + ∂Ψ⁰/∂n_i) at (T, ρ), J/mol, in this mixture's gauge (its components'
    /// z-weighted). With α⁰ = ln ρ + Σ z_i w_i (c_i + ln z_i), w_i = R_i/R and c_i = α⁰_oi(T, ρ) − ln ρ the component's
    /// own, Ψ⁰ = n α⁰ gives ∂Ψ⁰/∂n_i = ln ρ + 1 + w_i (c_i + ln z_i + 1) − Σ_j z_j w_j.
    pub fn chemical_potentials(&self, t: f64, rho: f64) -> Result<Vec<f64>, Error> {
        let residual = self.composition_at(t, rho, Order::One)?;
        let ln_rho = math::ln(rho);
        let weighted: f64 = self.z.iter().zip(self.r_ratio.iter()).map(|(z, w)| z * w).sum();
        let rt = self.fluid.gas_constant() * t;
        self.fluid
            .fluids()
            .zip(self.z.iter().zip(self.r_ratio.iter()))
            .enumerate()
            .map(|(i, (f, (z, w)))| {
                let eos =
                    f.model().helmholtz().ok_or(Error::NoModel { prop: Prop::Constant(Constant::GasConstant) })?;
                let c = eos.ideal(t, rho, Order::One).get(0, 0).unwrap_or(f64::NAN) - ln_rho;
                let ideal = ln_rho + 1.0 + w * (c + math::ln(*z) + 1.0) - weighted;
                Ok(rt * (residual.psi(&[Var::N(i)]) + ideal))
            })
            .collect()
    }

    /// The state at (T, ρ) as `phase`, under `opts`'s domain policy: outside the limits refused, or extrapolated and so
    /// flagged.
    fn state(&self, t: f64, rho: f64, phase: Phase, opts: &FlashOptions) -> Result<State, Error> {
        self.state_of(&self.z, self.info.molar_mass(), t, rho, phase, opts)
    }

    /// The state of a phase of mole fractions `x` and molar mass `molar_mass` at (T, ρ), under this mixture's key, so
    /// that two of them make a split.
    fn state_of(
        &self,
        x: &[f64],
        molar_mass: f64,
        t: f64,
        rho: f64,
        phase: Phase,
        opts: &FlashOptions,
    ) -> Result<State, Error> {
        let inside = self.limits.check_t(t).is_ok();
        if !inside && opts.domain() == DomainPolicy::Enforce {
            self.limits.check_t(t)?;
        }
        let derivs = self.ideal_at(x, t, rho, Order::Two) + self.fluid.residual_at(x, t, rho, Order::Two);
        let total = derivs.bundle().ok_or(Error::InvalidInput { quantity: "density", value: rho })?;
        let r = self.fluid.gas_constant();
        let state = State::from_total(self.info.key(), t, rho, r, molar_mass, phase, &total)?;
        Ok(if inside { state } else { state.mark_extrapolated() })
    }

    /// The molar mass of mole fractions `x`, kg/mol.
    fn molar_mass_of(&self, x: &[f64]) -> f64 {
        self.fluid.fluids().zip(x).map(|(f, x)| x * f.info().molar_mass()).sum()
    }

    /// The equilibrium at (T, p) (PLAN.md M13.8): the tangent-plane test of the feed on its stable root, then the split
    /// from the unstable trial phase (Wilson's K if that collapses).
    pub fn equilibrium(&self, t: f64, p: f64) -> Result<Equilibrium, Error> {
        let feed = equilibrium::at_t_p(self, t, p, &self.z, Order::One)?;
        let Stability::Unstable(w) = equilibrium::stability(self, t, p, &self.z, &feed)? else {
            return Ok(Equilibrium::Single { rho: feed.rho });
        };
        // The trial phase is the incipient one: the vapour where it is the lighter.
        let lighter = equilibrium::at_t_p(self, t, p, &w, Order::One)?.rho < feed.rho;
        let from_trial: Vec<f64> =
            w.iter().zip(self.z.iter()).map(|(w, z)| if lighter { math::ln(w / z) } else { math::ln(z / w) }).collect();
        for ln_k in [from_trial, equilibrium::wilson(self, t, p)] {
            if let Some(s) = equilibrium::split(self, t, p, &self.z, ln_k)? {
                return Ok(Equilibrium::Split {
                    beta: s.beta,
                    x: s.x,
                    y: s.y,
                    rho_liquid: s.liquid.rho,
                    rho_vapour: s.vapour.rho,
                });
            }
        }
        Err(Error::NoConvergence { strategy: phasekit_core::Strategy::Vle, iterations: 0 })
    }

    /// The bubble or dew point at a given T or p (PLAN.md M13.9): the other of T and p, the feed's density and the
    /// incipient phase's fractions and density.
    pub fn saturation(&self, side: Side, given: Given) -> Result<Saturation, Error> {
        saturation::saturation(self, side, given)
    }

    /// Two phases at vapour fraction `beta` in [0, 1] and a given T or p (PLAN.md M13.10): the other of T and p and both
    /// phases, adding up to the feed (ROT-182). β = 0 is the bubble point, β = 1 the dew point.
    pub fn coexisting(&self, beta: f64, given: Given) -> Result<Coexisting, Error> {
        if !(0.0..=1.0).contains(&beta) {
            return Err(Error::InvalidInput { quantity: "vapour fraction", value: beta });
        }
        saturation::coexisting(self, beta, given)
    }

    /// The two-phase state of `c`, the liquid first: its molar quality β, its mass quality from each phase's molar mass.
    fn coexisting_state(&self, c: &Coexisting, opts: &FlashOptions) -> Result<State, Error> {
        let liquid = self.state_of(&c.x, self.molar_mass_of(&c.x), c.t, c.rho_liquid, Phase::Liquid, opts)?;
        let vapour = self.state_of(&c.y, self.molar_mass_of(&c.y), c.t, c.rho_vapour, Phase::Gas, opts)?;
        State::from_split(liquid, vapour, Quality::new(c.beta)?, c.t, c.p)
    }

    /// The coexisting phases at a mass quality `q` and `given`: the molar β where Q(β) = β M″/(β M″ + (1 − β) M′) is q,
    /// each phase's molar mass M′, M″ from its solve, by the secant method from β = q and the β that q gives with the
    /// first solve's molar masses, to |Q(β) − q| ≤ 4 ε.
    fn at_mass_quality(&self, q: f64, given: Given) -> Result<Coexisting, Error> {
        let masses = |c: &Coexisting| (self.molar_mass_of(&c.x), self.molar_mass_of(&c.y));
        let at = |beta: f64| -> Result<(f64, Coexisting), Error> {
            let c = self.coexisting(beta, given)?;
            Ok((mass_fraction(beta, masses(&c)) - q, c))
        };
        let (mut b0, (mut f0, first)) = (q, at(q)?);
        if f0.abs() <= 4.0 * f64::EPSILON {
            return Ok(first);
        }
        let mut b1 = molar_fraction(q, masses(&first));
        for _ in 0..MASS_STEPS {
            let (f1, c) = at(b1)?;
            if f1.abs() <= 4.0 * f64::EPSILON || f1 == f0 {
                return Ok(c);
            }
            (b0, f0, b1) = (b1, f1, secant((b0, f0), (b1, f1)));
        }
        Err(Error::NoConvergence { strategy: phasekit_core::Strategy::Vle, iterations: MASS_STEPS as u16 })
    }

    /// QT and PQ (PLAN.md M13.9, M13.10): the two phases at the given quality, molar or by mass, and T or p. Under
    /// `DomainPolicy::Enforce` a given T outside the limits is refused before the solve.
    fn quality_input(&self, input: &NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        let (q, given) = match (input.pair(), input.values()) {
            (Pair::QT, (q, t)) => (q, Given::T(t)),
            (_, (p, q)) => (q, Given::P(p)),
        };
        if let (Given::T(t), DomainPolicy::Enforce) = (given, opts.domain()) {
            self.limits.check_t(t)?;
        }
        // A mass quality of 0 or 1 is the molar one: `at_mass_quality` returns its first point.
        let c = match input.quality_basis() {
            Basis::Mass => self.at_mass_quality(q, given)?,
            _ => self.coexisting(q, given)?,
        };
        self.coexisting_state(&c, opts)
    }

    /// DT without a phase hint (PLAN.md M13.10): the homogeneous state at (T, ρ) where it is stable, mechanically and by
    /// Michelsen's tangent-plane test at its own pressure; else the isochoric split, found in β between the bubble and
    /// dew points at T, or where T has none, in p.
    fn dt(&self, t: f64, rho: f64, opts: &FlashOptions) -> Result<State, Error> {
        let b = (self.residual(t, rho, Order::Two) + Derivs::IDEAL_DELTA)
            .bundle()
            .ok_or(Error::InvalidInput { quantity: "density", value: rho })?;
        let p = pressure(self.fluid.gas_constant(), t, rho, &b);
        if self.is_stable(t, rho, p, &b)? {
            return self.state(t, rho, self.label(t, rho, p), opts);
        }
        if let Some(c) = saturation::isochoric(self, t, rho)? {
            return self.coexisting_state(&c, opts);
        }
        let (p, e) = self.isochoric(t, rho, p)?;
        self.equilibrium_state(t, p, e, opts)
    }

    /// Whether the feed's homogeneous state at (T, ρ), of pressure `p` and total derivatives `b`, is stable: mechanically
    /// and by Michelsen's tangent-plane test at that pressure (DT's check).
    fn is_stable(&self, t: f64, rho: f64, p: f64, b: &Bundle) -> Result<bool, Error> {
        if !mechanically_stable(b) {
            return Ok(false);
        }
        let feed = equilibrium::AtTp::new(self, t, p, rho, &self.z, Order::One)?;
        Ok(equilibrium::stability(self, t, p, &self.z, &feed)? == Stability::Stable)
    }

    /// The isochoric split at (T, ρ) where T has no bubble or dew point: the equilibrium at the p where the PT flash's
    /// bulk molar volume is 1/ρ. That volume
    /// falls with p along the equilibrium isotherm, through the dome too, so the p is one: found in ln p from the
    /// homogeneous pressure `p_h` (1 bar where it is not positive), its bracket widened by 4 at a time, then closed by
    /// the Illinois regula falsi to `ISOCHORIC_WIDTH` in ln p. Each split after the first starts from the last one's K.
    fn isochoric(&self, t: f64, rho: f64, p_h: f64) -> Result<(f64, Equilibrium), Error> {
        let mut last_k: Option<Vec<f64>> = None;
        // ln(v_eq ρ) at ln p: positive where p lies below the solution.
        let mut g = |u: f64| -> Result<(f64, Equilibrium), Error> {
            let p = math::exp(u);
            let e = match last_k.take().map(|k| equilibrium::split(self, t, p, &self.z, k)) {
                Some(Ok(Some(s))) => Equilibrium::Split {
                    beta: s.beta,
                    x: s.x,
                    y: s.y,
                    rho_liquid: s.liquid.rho,
                    rho_vapour: s.vapour.rho,
                },
                _ => self.equilibrium(t, p)?,
            };
            if let Equilibrium::Split { x, y, .. } = &e {
                last_k = Some(saturation::ln_k_of(x, y));
            }
            Ok((math::ln(bulk_volume(&e) * rho), e))
        };
        let (lo, hi) = bracket(isochoric_start(p_h), &mut g)?;
        let found = illinois(lo, hi, &mut g, (ISOCHORIC_WIDTH, ISOCHORIC_ITERATIONS))?;
        Ok((math::exp(found.x), found.at))
    }

    /// The state of an equilibrium at (T, p): a single phase labelled from the critical point, or the split, the liquid
    /// first; a liquid-liquid split refused (MX4).
    fn equilibrium_state(&self, t: f64, p: f64, e: Equilibrium, opts: &FlashOptions) -> Result<State, Error> {
        match e {
            Equilibrium::Single { rho } => self.state(t, rho, self.label(t, rho, p), opts),
            Equilibrium::Split { beta, x, y, rho_liquid, rho_vapour } => {
                if self.critical_point().is_some_and(|c| rho_liquid.min(rho_vapour) > c.rho) {
                    return Err(Error::Unsupported { pair: Pair::PT });
                }
                let liquid = self.state_of(&x, self.molar_mass_of(&x), t, rho_liquid, Phase::Liquid, opts)?;
                let vapour = self.state_of(&y, self.molar_mass_of(&y), t, rho_vapour, Phase::Gas, opts)?;
                State::from_split(liquid, vapour, Quality::new(beta)?, t, p)
            }
        }
    }

    /// A single phase's label (ROT-009): at or above the critical temperature as a pure fluid's (supercritical above
    /// the critical pressure, else a supercritical gas); below it liquid where ρ > ρ_c(z) (a supercritical liquid above
    /// p_c), gas otherwise. v8.0.0 compares ρ with ρ_r(z). Without a critical point, ρ_r(z) decides.
    fn label(&self, t: f64, rho: f64, p: f64) -> Phase {
        label(self.critical_point().as_ref(), self.fluid.reducing_state(&self.z).1, (t, rho, p))
    }

    /// The PT flash without a hint: the stable single phase, labelled, or the split's two phases; a split whose two
    /// phases are both denser than the critical point is liquid-liquid, refused as `Unsupported` (MX4).
    fn pt(&self, t: f64, p: f64, opts: &FlashOptions) -> Result<State, Error> {
        let e = self.equilibrium(t, p)?;
        self.equilibrium_state(t, p, e, opts)
    }
}

/// A mixture's phase equilibrium at (T, p) ([`Mixture::equilibrium`]).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Equilibrium {
    /// One phase, stable, on its stable root.
    Single {
        /// Molar density, mol/m³.
        rho: f64,
    },
    /// Two phases at equal fugacities.
    Split {
        /// The vapour's molar fraction of the whole, in (0, 1).
        beta: f64,
        /// The liquid's mole fractions.
        x: Vec<f64>,
        /// The vapour's mole fractions.
        y: Vec<f64>,
        /// The liquid's molar density, mol/m³.
        rho_liquid: f64,
        /// The vapour's molar density, mol/m³.
        rho_vapour: f64,
    },
}

/// [`Mixture`]'s label of a single phase at (T, ρ, p) (ROT-009), from its critical point `critical` and, without one, its
/// reducing density `rho_r`.
fn label(critical: Option<&CriticalPoint>, rho_r: f64, (t, rho, p): (f64, f64, f64)) -> Phase {
    match critical {
        Some(c) if t == c.t && rho == c.rho => Phase::CriticalPoint,
        Some(c) if t >= c.t => {
            if p > c.p {
                Phase::Supercritical
            } else {
                Phase::SupercriticalGas
            }
        }
        Some(c) if rho > c.rho => {
            if p > c.p {
                Phase::SupercriticalLiquid
            } else {
                Phase::Liquid
            }
        }
        Some(_) => Phase::Gas,
        None if rho > rho_r => Phase::Liquid,
        None => Phase::Gas,
    }
}

/// Whether a homogeneous state of total derivatives `b` is mechanically stable: p > 0 and (∂p/∂ρ)_T > 0, so Z > 0 and
/// 2Z + δ²α_δδ − 1 > 0 (ρRT is positive).
fn mechanically_stable(b: &Bundle) -> bool {
    b.a01 > 0.0 && 2.0 * b.a01 + b.a02 > 0.0
}

/// The secant step from (β₀, f₀) and (β₁, f₁) toward f = 0, kept inside (0, 1): at least the smallest positive
/// double, at most 1 − ε.
fn secant((b0, f0): (f64, f64), (b1, f1): (f64, f64)) -> f64 {
    (b1 - f1 * (b1 - b0) / (f1 - f0)).clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON)
}

/// The mass quality of molar vapour fraction β whose liquid's and vapour's molar masses are `m`: βM″/(βM″ + (1 − β)M′).
fn mass_fraction(beta: f64, (m_liquid, m_vapour): (f64, f64)) -> f64 {
    beta * m_vapour / (beta * m_vapour + (1.0 - beta) * m_liquid)
}

/// The molar vapour fraction of mass quality q whose phases' molar masses are `m`: qM′/(qM′ + (1 − q)M″).
fn molar_fraction(q: f64, (m_liquid, m_vapour): (f64, f64)) -> f64 {
    q * m_liquid / (q * m_liquid + (1.0 - q) * m_vapour)
}

/// A bracket in ln p of the isochoric split's `g`, which falls as p rises, from `start`: its ends stepped out by a
/// factor of 4 in p, at most `ISOCHORIC_STEPS` times, until g is positive at the lower and negative at the upper.
fn bracket(
    start: f64,
    g: &mut impl FnMut(f64) -> Result<(f64, Equilibrium), Error>,
) -> Result<(End<Equilibrium>, End<Equilibrium>), Error> {
    let first = g(start)?;
    let (mut lo, mut hi) = (End::new(start, first.clone()), End::new(start, first));
    let widen = math::ln(4.0);
    for steps in 0..=ISOCHORIC_STEPS {
        if lo.value > 0.0 && hi.value < 0.0 {
            return Ok((lo, hi));
        }
        if steps == ISOCHORIC_STEPS {
            break;
        }
        if lo.value <= 0.0 {
            lo = End::new(lo.x - widen, g(lo.x - widen)?);
        }
        if hi.value >= 0.0 {
            hi = End::new(hi.x + widen, g(hi.x + widen)?);
        }
    }
    Err(Error::NoConvergence { strategy: phasekit_core::Strategy::Vle, iterations: ISOCHORIC_STEPS as u16 })
}

/// The pressure of a homogeneous state at (T, ρ) of total derivatives `b`: ρRT(1 + δα^r_δ), Z being `b.a01`.
fn pressure(r: f64, t: f64, rho: f64, b: &Bundle) -> f64 {
    rho * r * t * b.a01
}

/// The bulk molar volume of an equilibrium: 1/ρ of one phase, (1 − β)/ρ′ + β/ρ″ of two.
fn bulk_volume(e: &Equilibrium) -> f64 {
    match e {
        Equilibrium::Single { rho } => 1.0 / rho,
        Equilibrium::Split { beta, rho_liquid, rho_vapour, .. } => (1.0 - beta) / rho_liquid + beta / rho_vapour,
    }
}

/// Where the isochoric split's search in ln p starts: the homogeneous pressure, or 1 bar where that is not positive and
/// finite.
fn isochoric_start(p_h: f64) -> f64 {
    math::ln(if p_h > 0.0 && p_h.is_finite() { p_h } else { 1e5 })
}

/// The components' limits' intersection: the highest T_min, the lowest T_max and p_max, and no triple point.
fn intersection(fluids: &[&Fluid]) -> Result<Limits, Error> {
    let limits: Vec<&Limits> = fluids.iter().filter_map(|f| f.model().limits()).collect();
    let t_min = limits.iter().map(|l| l.t_min()).fold(0.0, f64::max);
    let t_max = limits.iter().map(|l| l.t_max()).fold(f64::INFINITY, f64::min);
    let p_max = limits.iter().map(|l| l.p_max()).fold(f64::INFINITY, f64::min);
    Limits::new(t_min.max(f64::MIN_POSITIVE), t_max, p_max)
}

impl HelmholtzModel for Mixture {
    fn gas_constant(&self) -> f64 {
        self.fluid.gas_constant()
    }

    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        self.fluid.residual_at(&self.z, t, rho, order)
    }

    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
        self.ideal_at(&self.z, t, rho, order)
    }

    /// The molar volume's lower bound at T, the components' z-weighted: 1/Σ z_i/ρ_max,i(T).
    fn rho_max(&self, t: f64) -> f64 {
        self.rho_max_at(&self.z, t)
    }

    fn zero_density(&self, t: f64) -> Option<Virials> {
        self.fluid.virials(&self.z, t)
    }

    fn reducing(&self) -> Option<Reducing> {
        let (t, rho) = self.fluid.reducing_state(&self.z);
        Some(Reducing { t, rho, p: None })
    }
}

impl Mixture {
    /// α⁰ at mole fractions `x` of this mixture's components (a phase's; [`HelmholtzModel::ideal`] at z).
    pub(crate) fn ideal_at(&self, x: &[f64], t: f64, rho: f64, order: Order) -> Derivs {
        let ln_rho = math::ln(rho);
        let parts: Vec<(f64, f64, Derivs)> = self
            .fluid
            .fluids()
            .zip(x.iter().zip(self.r_ratio.iter()))
            .map(|(f, (z, r))| {
                let d = f.model().helmholtz().map(|m| m.ideal(t, rho, order));
                (*z * r, math::ln(*z), d.unwrap_or_else(|| Derivs::from_fn(order, |_, _| f64::NAN)))
            })
            .collect();
        Derivs::from_fn(order, |i, j| match (i, j) {
            (0, 0) => {
                let sum: f64 =
                    parts.iter().map(|(w, ln_z, d)| w * (d.get(0, 0).unwrap_or(f64::NAN) - ln_rho + ln_z)).sum();
                ln_rho + sum
            }
            (0, j) => Derivs::IDEAL_DELTA.get(0, j).unwrap_or(0.0),
            (i, 0) => parts.iter().map(|(w, _, d)| w * d.get(i, 0).unwrap_or(f64::NAN)).sum(),
            _ => 0.0,
        })
    }

    /// The molar volume's lower bound at T for mole fractions `x`, the components' x-weighted: 1/Σ x_i/ρ_max,i(T).
    pub(crate) fn rho_max_at(&self, x: &[f64], t: f64) -> f64 {
        let volume: f64 = self
            .fluid
            .fluids()
            .zip(x.iter())
            .map(|(f, x)| x / f.model().helmholtz().map_or(f64::NAN, |m| m.rho_max(t)))
            .sum();
        1.0 / volume
    }
}

impl ThermoModel for Mixture {
    fn info(&self) -> &FluidInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Pair::ALL.iter().fold(Capabilities::none(), |c, pair| c.with(*pair))
    }

    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        let (pair, (x, y)) = (input.pair(), input.values());
        if matches!(pair, Pair::QT | Pair::PQ) {
            return self.quality_input(&input, opts);
        }
        let Some(phase) = opts.phase_hint() else {
            return match pair {
                Pair::PT => self.pt(y, x, opts),
                Pair::DT => self.dt(y, x, opts),
                _ => self.outer(&input, opts),
            };
        };
        match pair {
            Pair::DT => self.state(y, x, phase, opts),
            Pair::PT => {
                let branch = match phase {
                    Phase::Liquid | Phase::SupercriticalLiquid => Branch::Liquid,
                    _ => Branch::Vapour,
                };
                let guess = opts.guess().map(|(_, rho)| rho);
                self.state(y, equilibrium::branch_root(self, y, x, branch, guess)?, phase, opts)
            }
            _ => Err(Error::Unsupported { pair }),
        }
    }

    fn limits(&self) -> Option<&Limits> {
        Some(&self.limits)
    }

    /// The vapour-liquid critical point, as the model's own (`CriticalOrigin::Model`): of the critical points at
    /// positive pressure, the one at the highest temperature, on the stability limit's outermost branch. The others
    /// at positive pressure lie on inner branches (Methane/n-Propane's at 240 K, below its 314 K one; v8.0.0's
    /// `all_critical_points` reports only the outer one); M13.12's envelope traces through the one chosen.
    fn critical_point(&self) -> Option<CriticalPoint> {
        self.critical_points().iter().filter(|c| c.p > 0.0).max_by(|a, b| a.t.total_cmp(&b.t)).map(|c| CriticalPoint {
            t: c.t,
            p: c.p,
            rho: c.rho,
            origin: CriticalOrigin::Model,
        })
    }

    fn helmholtz(&self) -> Option<&dyn HelmholtzModel> {
        Some(self)
    }
}

/// The fluid of `components` (names, alias or CAS, with mole fractions) in `registry`: zero fractions trimmed
/// (ROT-192), the rest validated (ROT-193); one component left is that pure fluid itself, from `registry` (bitwise,
/// under any gauge; ROT-099, ROT-208), two or more a [`Mixture`] on `data_set`'s library.
pub fn mixture(registry: &Registry, components: &[(&str, f64)], data_set: DataSet) -> Result<Fluid, Error> {
    let (names, fractions) = trimmed(components)?;
    if let [one] = names[..] {
        return registry.resolve(one);
    }
    let mixture = Mixture::new(MultiFluid::new(registry, &names, data_set)?, &fractions)?;
    Ok(Fluid::new(Arc::new(mixture)))
}

/// GERG-2008's fluid of `components` (names, alias or CAS in `registry`, with mole fractions; PLAN.md M13.5), trimmed
/// and validated as [`mixture`]'s: one component left is GERG's pure fluid, `GERG2008::<name>`, bit for bit the one
/// [`crate::with_gerg2008`] registers; two or more a [`Mixture`] on [`MultiFluid::gerg2008`].
pub fn gerg2008(registry: &Registry, components: &[(&str, f64)]) -> Result<Fluid, Error> {
    let (names, fractions) = trimmed(components)?;
    if let [one] = names[..] {
        let (i, fluid) = gerg::component(registry, one)?;
        return Ok(Fluid::new(Arc::new(gerg::pure_fluid(i, &fluid)?)));
    }
    let mixture = Mixture::new(MultiFluid::gerg2008(registry, &names)?, &fractions)?;
    Ok(Fluid::new(Arc::new(mixture)))
}

/// `components` validated (ROT-193), its zero fractions trimmed (ROT-192): the names and fractions left.
fn trimmed<'a>(components: &[(&'a str, f64)]) -> Result<(Vec<&'a str>, Vec<f64>), Error> {
    let z: Vec<f64> = components.iter().map(|(_, x)| *x).collect();
    validated(&z)?;
    Ok(components.iter().filter(|(_, x)| *x != 0.0).map(|&(n, x)| (n, x)).unzip())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The library's mixture of `components` at `z`, on `Parity`.
    fn library(components: &[&str], z: &[f64]) -> Mixture {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        let model = MultiFluid::new(&registry, components, DataSet::Parity).unwrap_or_else(|e| panic!("{e:?}"));
        Mixture::new(model, z).unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// A homogeneous state is mechanically stable where Z > 0 and 2Z + δ²α_δδ − 1 > 0, both strictly.
    #[test]
    fn mechanical_stability_needs_positive_p_and_slope() {
        let b = |a01: f64, a02: f64| Bundle { a00: 0.0, a10: 0.0, a01, a20: 0.0, a11: 0.0, a02 };
        assert!(mechanically_stable(&b(1.0, -1.0)) && mechanically_stable(&b(0.5, -0.9)));
        assert!(!mechanically_stable(&b(0.0, 1.0)) && !mechanically_stable(&b(-0.1, 5.0)));
        assert!(!mechanically_stable(&b(0.5, -1.0)) && !mechanically_stable(&b(0.5, -2.0)));
    }

    /// The mass quality of a molar β and its inverse: β = 0.5 of phases of 30 and 50 g/mol is a mass quality of 0.625,
    /// which gives back 0.5; each inverts the other within 1e-15 across [0, 1], whose ends are themselves.
    #[test]
    fn mass_and_molar_qualities_invert_each_other() {
        let m = (0.03, 0.05);
        assert!((mass_fraction(0.5, m) - 0.625).abs() <= 1e-15 && (molar_fraction(0.625, m) - 0.5).abs() <= 1e-15);
        for beta in [0.0, 0.1, 0.4, 0.9, 1.0] {
            assert!((molar_fraction(mass_fraction(beta, m), m) - beta).abs() <= 1e-15, "{beta}");
        }
        assert_eq!((mass_fraction(0.0, m), mass_fraction(1.0, m), molar_fraction(1.0, m)), (0.0, 1.0, 1.0));
    }

    /// The secant step lands on the line's root, kept inside (0, 1): from (0.5, −0.5) and (0.9, −0.1) on f = β − 1 it
    /// would land on 1 and stops at 1 − ε, on f = β + 1 at −1 and stops at the smallest positive double.
    #[test]
    fn secant_steps_stay_inside_the_unit_interval() {
        assert_eq!(secant((0.25, -0.25), (0.75, 0.25)), 0.5);
        assert_eq!(secant((0.5, -0.5), (0.9, -0.1)), 1.0 - f64::EPSILON);
        assert_eq!(secant((0.5, 1.5), (0.9, 1.9)), f64::MIN_POSITIVE);
    }

    /// The isochoric split's bracket in ln p, of a g that falls through 0 at `root`, from `start`: its ends and the
    /// evaluations it spent.
    fn bracketed(root: f64, start: f64) -> (Result<(f64, f64), Error>, usize) {
        let mut calls = 0;
        let mut g = |u: f64| {
            calls += 1;
            Ok((root - u, Equilibrium::Single { rho: u }))
        };
        let ends = bracket(start, &mut g).map(|(lo, hi)| (lo.x, hi.x));
        (ends, calls)
    }

    /// The bracket widens by a factor of 4 in p at a time, each end only while g has the wrong sign there and 0 counting
    /// as wrong: from a start below the root its upper end rises, from above its lower end falls, from the root both move
    /// once, an end that lands on the root moves on. A g that never falls below 0 is refused after `ISOCHORIC_STEPS`.
    #[test]
    fn the_isochoric_bracket_widens_by_four() {
        let w = math::ln(4.0);
        let near = |got: (Result<(f64, f64), Error>, usize), want: (f64, f64, usize)| match got {
            (Ok((lo, hi)), calls) => (lo - want.0).abs() <= 1e-14 && (hi - want.1).abs() <= 1e-14 && calls == want.2,
            _ => false,
        };
        assert!(near(bracketed(3.0, 0.0), (0.0, 3.0 * w, 4)) && near(bracketed(-3.0, 0.0), (-3.0 * w, 0.0, 4)));
        assert!(near(bracketed(0.0, 0.0), (-w, w, 3)));
        assert!(near(bracketed(-w, 0.0), (-2.0 * w, 0.0, 3)) && near(bracketed(w, 0.0), (0.0, 2.0 * w, 3)));
        let mut calls = 0;
        let mut never = |u: f64| {
            calls += 1;
            Ok((1.0, Equilibrium::Single { rho: u }))
        };
        let refused = bracket(0.0, &mut never).map(|(lo, hi)| (lo.x, hi.x));
        let want = Err(Error::NoConvergence { strategy: phasekit_core::Strategy::Vle, iterations: 40 });
        assert!(refused == want && calls == 1 + ISOCHORIC_STEPS, "{refused:?} {calls}");
    }

    /// The homogeneous pressure ρRTZ; the bulk volume of one phase and of two; the isochoric search's start at the
    /// homogeneous pressure where it is positive and finite, else at 1 bar.
    #[test]
    fn the_isochoric_search_s_pieces() {
        let b = Bundle { a00: 0.0, a10: 0.0, a01: 0.5, a20: 0.0, a11: 0.0, a02: 0.0 };
        assert_eq!(pressure(8.0, 300.0, 2.0, &b), 2400.0);
        let split = Equilibrium::Split { beta: 0.25, x: vec![], y: vec![], rho_liquid: 0.5, rho_vapour: 0.125 };
        assert_eq!((bulk_volume(&Equilibrium::Single { rho: 4.0 }), bulk_volume(&split)), (0.25, 3.5));
        assert_eq!(isochoric_start(2e6), math::ln(2e6));
        let bar = math::ln(1e5);
        assert!([0.0, -1.0, f64::NAN, f64::INFINITY].iter().all(|p| isochoric_start(*p) == bar));
    }

    /// DT's check: a supercritical gas is stable; the same mixture inside its dome is not, by Michelsen's test (a vapour
    /// 5 % denser than its dew point's at 200 K, mechanically stable), nor mechanically where (∂p/∂ρ)_T < 0 (inside the
    /// spinodal), which needs no tangent-plane test (library Methane/Ethane at 50/50).
    #[test]
    fn dt_s_stability_is_mechanical_then_michelsen_s() {
        let m = library(&["Methane", "Ethane"], &[0.5, 0.5]);
        let check = |t: f64, rho: f64| {
            let b = (m.residual(t, rho, Order::Two) + Derivs::IDEAL_DELTA).bundle().unwrap_or_else(|| panic!("{rho}"));
            (m.is_stable(t, rho, pressure(m.fluid.gas_constant(), t, rho, &b), &b), mechanically_stable(&b))
        };
        assert_eq!(check(300.0, 1000.0), (Ok(true), true));
        let dew = m.saturation(Side::Dew, Given::T(200.0)).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(check(200.0, 1.05 * dew.rho), (Ok(false), true));
        let spinodal = (1..60).map(|k| 250.0 * f64::from(k)).find(|rho| !check(200.0, *rho).1);
        let spinodal = spinodal.unwrap_or_else(|| panic!("no mechanically unstable density"));
        assert_eq!(check(200.0, spinodal).0, Ok(false));
    }

    /// Above the critical temperature, where T has two dew points and no bubble point, the isochoric split in β has no
    /// bubble point to start from, and DT inside the dome finds the split in p: library Methane/Ethane at 85/15, 1.01 T_c
    /// (220.15 K), the bulk density of the PT split at 5 MPa gives back its p within 1e-9 and its β within 1e-8.
    #[test]
    fn dt_above_the_critical_temperature_is_found_in_p() {
        let m = library(&["Methane", "Ethane"], &[0.85, 0.15]);
        let (t, p) = (220.148, 5e6);
        let Ok(Equilibrium::Split { beta, rho_liquid, rho_vapour, .. }) = m.equilibrium(t, p) else {
            panic!("no split")
        };
        let rho = 1.0 / ((1.0 - beta) / rho_liquid + beta / rho_vapour);
        assert_eq!(saturation::isochoric(&m, t, rho), Ok(None));
        let state = m.dt(t, rho, &FlashOptions::new()).unwrap_or_else(|e| panic!("{e:?}"));
        let q = state.quality().unwrap_or(f64::NAN);
        assert!((state.p() / p - 1.0).abs() <= 1e-9 && (q - beta).abs() <= 1e-8, "{state:?}");
    }

    /// DT labels a stable single phase from its own pressure: above T_c, at the densities PT gives at 0.9 p_c and
    /// 1.1 p_c, a supercritical gas and a supercritical fluid (library Methane/Ethane at 85/15, 1.2 T_c).
    #[test]
    fn dt_labels_from_the_state_s_pressure() {
        let m = library(&["Methane", "Ethane"], &[0.85, 0.15]);
        let c = m.critical_point().unwrap_or_else(|| panic!("critical"));
        let t = 1.2 * c.t;
        for (share, phase) in [(0.9, Phase::SupercriticalGas), (1.1, Phase::Supercritical)] {
            let Ok(Equilibrium::Single { rho }) = m.equilibrium(t, share * c.p) else { panic!("{share}") };
            let state = m.dt(t, rho, &FlashOptions::new()).unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(state.phase(), phase, "{share}");
        }
    }

    /// ROT-009's labels: at the critical point itself `CriticalPoint`; at or above T_c supercritical above p_c and a
    /// supercritical gas at or below it; below T_c a supercritical liquid above p_c and a liquid at or below it where ρ >
    /// ρ_c, a gas where ρ ≤ ρ_c; without a critical point liquid where ρ > ρ_r, else gas.
    #[test]
    fn single_phases_are_labelled_from_the_critical_point() {
        let c = CriticalPoint { t: 250.0, p: 5e6, rho: 9000.0, origin: CriticalOrigin::Model };
        let at = |t: f64, rho: f64, p: f64| label(Some(&c), 1.0, (t, rho, p));
        assert_eq!(at(250.0, 9000.0, 5e6), Phase::CriticalPoint);
        assert_eq!(
            (at(250.0, 9000.1, 5e6), at(250.0, 8999.9, 5e6 + 1.0)),
            (Phase::SupercriticalGas, Phase::Supercritical)
        );
        assert_eq!((at(260.0, 100.0, 5e6), at(260.0, 100.0, 6e6)), (Phase::SupercriticalGas, Phase::Supercritical));
        assert_eq!((at(240.0, 9000.1, 5e6), at(240.0, 9000.1, 5e6 + 1.0)), (Phase::Liquid, Phase::SupercriticalLiquid));
        assert_eq!((at(240.0, 9000.0, 6e6), at(240.0, 100.0, 1e5)), (Phase::Gas, Phase::Gas));
        assert_eq!(
            (label(None, 7000.0, (240.0, 7000.1, 1e5)), label(None, 7000.0, (240.0, 7000.0, 1e5))),
            (Phase::Liquid, Phase::Gas)
        );
    }
}
