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
//! branch through core's public density solve (M13.1). Any other input is `Unsupported`.

use std::sync::{Arc, OnceLock};

use phasekit_core::density::Branch;
use phasekit_core::{
    Capabilities, Constant, CriticalOrigin, CriticalPoint, DataSet, DataTerms, Derivs, DomainPolicy, Error,
    FlashOptions, Fluid, FluidInfo, HelmholtzModel, Limits, ModelKey, NativeInput, Order, Pair, Phase, Prop, Reducing,
    Registry, Source, State, ThermoModel, Virials, math,
};

use crate::critical::{CriticalState, Criticality, critical_points};
use crate::equilibrium::{self, Stability};
use crate::saturation::{self, Given, Saturation, Side};
use crate::{Composition, MixtureModel, MultiFluid, Var, gerg};

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

    /// A saturated state, Q = 0 at a bubble point and 1 at a dew point: the feed and the incipient phase, the liquid
    /// first, as a split.
    fn saturated(&self, side: Side, given: Given, opts: &FlashOptions) -> Result<State, Error> {
        let s = self.saturation(side, given)?;
        let feed = (self.z.to_vec(), s.rho);
        let incipient = (s.incipient.clone(), s.rho_incipient);
        let ((x, rho_l), (y, rho_v), q) = match side {
            Side::Bubble => (feed, incipient, 0.0),
            Side::Dew => (incipient, feed, 1.0),
        };
        let liquid = self.state_of(&x, self.molar_mass_of(&x), s.t, rho_l, Phase::Liquid, opts)?;
        let vapour = self.state_of(&y, self.molar_mass_of(&y), s.t, rho_v, Phase::Gas, opts)?;
        State::from_split(liquid, vapour, phasekit_core::Quality::new(q)?, s.t, s.p)
    }

    /// QT and PQ at Q = 0 (a bubble point) or 1 (a dew point); a Q inside (0, 1) is not yet served (M13.10). Under
    /// `DomainPolicy::Enforce` a given T outside the limits is refused before the solve.
    fn quality_input(&self, pair: Pair, (a, b): (f64, f64), opts: &FlashOptions) -> Result<State, Error> {
        let (q, given) = match pair {
            Pair::QT => (a, Given::T(b)),
            _ => (b, Given::P(a)),
        };
        if let (Given::T(t), DomainPolicy::Enforce) = (given, opts.domain()) {
            self.limits.check_t(t)?;
        }
        if q == 0.0 {
            self.saturated(Side::Bubble, given, opts)
        } else if q == 1.0 {
            self.saturated(Side::Dew, given, opts)
        } else {
            Err(Error::Unsupported { pair })
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
        match self.equilibrium(t, p)? {
            Equilibrium::Single { rho } => self.state(t, rho, self.label(t, rho, p), opts),
            Equilibrium::Split { beta, x, y, rho_liquid, rho_vapour } => {
                if self.critical_point().is_some_and(|c| rho_liquid.min(rho_vapour) > c.rho) {
                    return Err(Error::Unsupported { pair: Pair::PT });
                }
                let liquid = self.state_of(&x, self.molar_mass_of(&x), t, rho_liquid, Phase::Liquid, opts)?;
                let vapour = self.state_of(&y, self.molar_mass_of(&y), t, rho_vapour, Phase::Gas, opts)?;
                State::from_split(liquid, vapour, phasekit_core::Quality::new(beta)?, t, p)
            }
        }
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
        Capabilities::none().with(Pair::DT).with(Pair::PT).with(Pair::QT).with(Pair::PQ)
    }

    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        let (pair, (x, y)) = (input.pair(), input.values());
        if matches!(pair, Pair::QT | Pair::PQ) {
            return self.quality_input(pair, (x, y), opts);
        }
        let Some(phase) = opts.phase_hint() else {
            return match pair {
                Pair::PT => self.pt(y, x, opts),
                _ => Err(Error::Unsupported { pair }),
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
