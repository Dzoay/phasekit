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
//! Until the stability test (M13.8) a flash needs a phase hint: DT is then the state at (T, ρ), PT the density on the
//! hinted branch through core's public density solve (M13.1). Any other input is `Unsupported`.

use std::sync::Arc;

use phasekit_core::density::{self, Branch};
use phasekit_core::{
    Capabilities, Constant, DataSet, DataTerms, Derivs, DomainPolicy, Error, FlashOptions, Fluid, FluidInfo,
    HelmholtzModel, Limits, ModelKey, NativeInput, Order, Pair, Phase, Prop, Reducing, Registry, Source, State,
    ThermoModel, Virials, math,
};

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
        Ok(Mixture { fluid, z: z.into(), r_ratio, info, limits })
    }

    /// The mole fractions, one per component.
    pub fn composition(&self) -> &[f64] {
        &self.z
    }

    /// The multi-fluid model.
    pub fn model(&self) -> &MultiFluid {
        &self.fluid
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
        let inside = self.limits.check_t(t).is_ok();
        if !inside && opts.domain() == DomainPolicy::Enforce {
            self.limits.check_t(t)?;
        }
        let total = (self.ideal(t, rho, Order::Two) + self.residual(t, rho, Order::Two)).bundle();
        let total = total.ok_or(Error::InvalidInput { quantity: "density", value: rho })?;
        let r = self.fluid.gas_constant();
        let state = State::from_total(self.info.key(), t, rho, r, self.info.molar_mass(), phase, &total)?;
        Ok(if inside { state } else { state.mark_extrapolated() })
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
        let ln_rho = math::ln(rho);
        let parts: Vec<(f64, f64, Derivs)> = self
            .fluid
            .fluids()
            .zip(self.z.iter().zip(self.r_ratio.iter()))
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

    /// The molar volume's lower bound at T, the components' z-weighted: 1/Σ z_i/ρ_max,i(T).
    fn rho_max(&self, t: f64) -> f64 {
        let volume: f64 = self
            .fluid
            .fluids()
            .zip(self.z.iter())
            .map(|(f, z)| z / f.model().helmholtz().map_or(f64::NAN, |m| m.rho_max(t)))
            .sum();
        1.0 / volume
    }

    fn zero_density(&self, t: f64) -> Option<Virials> {
        self.fluid.virials(&self.z, t)
    }

    fn reducing(&self) -> Option<Reducing> {
        let (t, rho) = self.fluid.reducing_state(&self.z);
        Some(Reducing { t, rho, p: None })
    }
}

impl ThermoModel for Mixture {
    fn info(&self) -> &FluidInfo {
        &self.info
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::none().with(Pair::DT).with(Pair::PT)
    }

    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        let (pair, (x, y)) = (input.pair(), input.values());
        let Some(phase) = opts.phase_hint() else { return Err(Error::Unsupported { pair }) };
        match pair {
            Pair::DT => self.state(y, x, phase, opts),
            Pair::PT => {
                let branch = match phase {
                    Phase::Liquid | Phase::SupercriticalLiquid => Branch::Liquid,
                    _ => Branch::Vapour,
                };
                let guess = opts.guess().map(|(_, rho)| rho);
                self.state(y, density::solve(self, y, x, branch, guess)?, phase, opts)
            }
            _ => Err(Error::Unsupported { pair }),
        }
    }

    fn limits(&self) -> Option<&Limits> {
        Some(&self.limits)
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
