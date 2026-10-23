//! Extended corresponding states (PLAN.md M8.8; map 05 U8, §3.2-§3.3): a fluid's η and λ from its reference fluid's
//! own forms at the conformal state (T₀, ρ₀), where the reference's α^r and Z equal the fluid's. The formulas are
//! CoolProp's, in its operation order (`TransportRoutines.cpp:1203-1373`), so the oracle agrees within `TransportEcs`
//! wherever it converges. The conformal solve is not: CoolProp's Newton differentiates in the reference's critical
//! point instead of its reducing state, swallows every exception and stops at |r| ≤ 1e-9 (map 05 R7); this one takes
//! the exact Jacobian, reports a failure as `NoConvergence`, and runs until the step reaches rounding.
//!
//! The reference fluid comes from the transport set's strong handles, resolved when the registry built its layer (E5);
//! its forms are read through `PureFluid`, so a provided model that is not one cannot serve as a reference.

use core::any::Any;

use super::conductivity::{olchowy_sengers, residual};
use super::viscosity::{TransportInputs, ViscosityStages, kinetic_theory, stages, sum};
use super::{
    ConductivityModel, ConductivityStages, EcsConductivity, EcsViscosity, LennardJones, OlchowySengers, ViscosityModel,
};
use crate::derivs::Order;
use crate::error::{Error, LoadError};
use crate::fluid::{Fluid, PureFluid};
use crate::math;
use crate::model::ThermoModel;
use crate::num::solve_small;
use crate::prop::Prop;
use crate::state::{Phase, SolvePath, State, Strategy};

/// The most Newton steps the conformal solve takes (CoolProp's limit).
const MAX_ITERATIONS: u16 = 50;
/// The residual norm at which the solve stops: within rounding of α^r and Z (a few ulps of a liquid's α^r).
const ROUNDING: f64 = 1e-13;
/// The residual norm a solve that stops short of `ROUNDING` must reach: CoolProp's (`TransportRoutines.cpp:1200`).
const TOLERANCE: f64 = 1e-9;

/// The reference fluid named `name` among `references`, as a Helmholtz fluid whose forms the ECS terms read.
fn reference<'a>(references: &'a [Fluid], name: &str) -> Result<&'a PureFluid, Error> {
    let named = |f: &&Fluid| f.info().name() == name || f.info().aliases().any(|a| a == name);
    let fluid = references.iter().find(named);
    let fluid =
        fluid.ok_or_else(|| LoadError::Format(format!("the ECS reference fluid {name} is not loaded").into()))?;
    let model: &dyn Any = fluid.model();
    let pure = model.downcast_ref::<PureFluid>();
    pure.ok_or_else(|| {
        LoadError::Format(format!("the ECS reference fluid {name} is not a Helmholtz fluid").into()).into()
    })
}

/// A conformal state on the reference's EOS: T₀, K, and ρ₀, mol/m³, with the Newton steps it took.
#[derive(Clone, Copy, Debug)]
struct Conformal {
    t: f64,
    rho: f64,
    iterations: u16,
}

/// The reference's residuals r = (α^r₀ − α^r, Z₀ − Z) at (t, ρ) and their Jacobian in (ln T₀, ln ρ₀): ∂α^r/∂ln T =
/// −τα^r_τ, ∂α^r/∂ln ρ = δα^r_δ, ∂Z/∂ln T = −τδα^r_δτ and ∂Z/∂ln ρ = δ²α^r_δδ + δα^r_δ, in the reference's own reducing
/// state.
fn residuals(reference: &PureFluid, target: (f64, f64), t: f64, rho: f64) -> ([f64; 2], [[f64; 2]; 2]) {
    let a = reference.eos().residual(t, rho, Order::Two);
    let at = |i, j| a.get(i, j).unwrap_or(f64::NAN);
    let (a00, a10, a01, a02, a11) = (at(0, 0), at(1, 0), at(0, 1), at(0, 2), at(1, 1));
    let r = [a00 - target.0, 1.0 + a01 - target.1];
    (r, [[-a10, a01], [-a11, a02 + a01]])
}

fn norm(r: [f64; 2]) -> f64 {
    math::sqrt(r[0] * r[0] + r[1] * r[1])
}

/// The conformal state of `x` on `reference`'s EOS, from CoolProp's guess T₀ = T/f, ρ₀ = ρ h with f = T_c/T_c0 and
/// h = ρ_c0/ρ_c, the models' own critical points (1 without them). Newton runs in ln T₀ and ln ρ₀, so every iterate is
/// positive; each step is halved, down to 1/512, until the residual falls. The solve ends once the residual is within
/// rounding (`ROUNDING`) or no step lowers it, and then succeeds only within CoolProp's tolerance, 1e-9: in a dilute gas
/// α^r and Z − 1 are both nearly B ρ, so the Jacobian is nearly singular and the iterates wander at the residual's
/// rounding floor without converging in T₀.
fn conformal_state(x: &TransportInputs<'_>, reference: &PureFluid) -> Result<Conformal, Error> {
    let (t, rho) = (x.t(), x.rho());
    let a = x.eos().residual(t, rho, Order::One);
    let target = (a.get(0, 0).unwrap_or(f64::NAN), 1.0 + a.get(0, 1).unwrap_or(f64::NAN));
    let (f, h) = match (x.critical(), reference.critical_point()) {
        (Some(c), Some(c0)) => (c.t / c0.t, c0.rho / c.rho),
        _ => (1.0, 1.0),
    };
    let (mut t0, mut rho0) = (t / f, rho * h);
    let fail = |iterations| Error::no_convergence(Strategy::Conformal, iterations);
    let done = |t0, rho0, residual: f64, iterations| {
        if residual <= TOLERANCE { Ok(Conformal { t: t0, rho: rho0, iterations }) } else { Err(fail(iterations)) }
    };
    let mut residual = f64::INFINITY;
    for iterations in 1..=MAX_ITERATIONS {
        let (r, jacobian) = residuals(reference, target, t0, rho0);
        residual = norm(r);
        if residual <= ROUNDING {
            return Ok(Conformal { t: t0, rho: rho0, iterations });
        }
        let v = solve_small(jacobian, [-r[0], -r[1]]).map_err(|_| fail(iterations))?;
        let mut fractions = core::iter::successors(Some(1.0), |frac: &f64| Some(frac / 2.0)).take(10);
        let step = fractions.find_map(|frac| {
            let (t_new, rho_new) = (t0 * math::exp(frac * v[0]), rho0 * math::exp(frac * v[1]));
            // Equivalent (M8.8): accepting a step that leaves the residual equal, not only one that lowers it, changes
            // only which iterate within rounding of the root the solve returns.
            // equivalent mutant: replace < with <=
            (norm(residuals(reference, target, t_new, rho_new).0) < residual).then_some((t_new, rho_new))
        });
        let Some(step) = step else { return done(t0, rho0, residual, iterations) };
        (t0, rho0) = step;
    }
    done(t0, rho0, residual, MAX_ITERATIONS)
}

/// ψ = Σ aᵢ (ρ/ρ_r)ᵗⁱ in the target's molar density.
fn psi(terms: &super::Series, rho_r: f64, x: &TransportInputs<'_>) -> f64 {
    sum(terms, x.rho() / rho_r)
}

/// The reference's single-phase state at (t, ρ), labelled gas as CoolProp imposes it, built from its EOS without a
/// flash (no domain check: `update_DmolarT_direct`).
fn reference_state(reference: &PureFluid, conformal: Conformal, rho: f64) -> Result<State, Error> {
    let eos = reference.eos();
    let total = eos.ideal(conformal.t, rho, Order::Two) + eos.residual(conformal.t, rho, Order::Two);
    let bundle =
        total.bundle().ok_or(Error::InvalidState { reason: "the reference returned a bundle below order 2" })?;
    let (r, m, key) = (eos.gas_constant(), reference.info().molar_mass(), reference.info().key());
    let path = SolvePath { strategy: Strategy::Conformal, iterations: conformal.iterations };
    State::single(key, conformal.t, rho, r, m, Phase::Gas, &bundle, path)
}

/// The kinetic-theory dilute viscosity of the target, Pa s, from the viscosity block's Lennard-Jones parameters.
fn eta_dilute(lj: LennardJones, x: &TransportInputs<'_>) -> f64 {
    kinetic_theory(lj, x.molar_mass(), x.t())
}

/// ECS viscosity at `x` (`TransportRoutines.cpp:1203-1264`): η = η₀ + η_ref(T₀, ρ₀ ψ) F_η with the reference's
/// background (initial-density and higher-order terms), F_η = √f h^(−2/3) √(M/M₀), f = T/T₀ and h = ρ₀/ρ. η₀ is the
/// target's kinetic-theory term; the reference's terms scale by F_η.
pub(crate) fn viscosity(
    ecs: &EcsViscosity,
    lj: LennardJones,
    x: &TransportInputs<'_>,
    references: &[Fluid],
) -> Result<ViscosityStages, Error> {
    let reference = reference(references, &ecs.reference)?;
    let psi = psi(&ecs.psi, ecs.psi_rho_r, x);
    let dilute = eta_dilute(lj, x);
    let conformal = conformal_state(x, reference)?;
    let state = reference_state(reference, conformal, conformal.rho * psi)?;
    let transport = reference.transport()?;
    let own = transport.record.viscosity.as_ref().ok_or(Error::NoModel { prop: Prop::Viscosity })?;
    let ViscosityModel::Staged(staged) = &own.model else {
        return Err(
            LoadError::Format(format!("the ECS reference {} has no staged viscosity", ecs.reference).into()).into()
        );
    };
    let inputs = TransportInputs::new(&state, reference.info().molar_mass(), reference.eos());
    let background = stages(staged, own.lennard_jones.parameters(), &inputs)?;
    let (f, h) = (x.t() / conformal.t, conformal.rho / x.rho());
    let f_eta = math::sqrt(f) * math::powf(h, -2.0 / 3.0) * math::sqrt(x.molar_mass() / reference.info().molar_mass());
    Ok(ViscosityStages {
        dilute,
        initial_density: background.initial_density * f_eta,
        residual: background.residual * f_eta,
        critical: 0.0,
    })
}

/// ECS conductivity at `x` (`TransportRoutines.cpp:1294-1373`): λ = λ_int + λ_dil + λ_ref(T₀, ρ₀ ψ) F_λ + λ_c with
/// λ_int = f_int η₀ (c_p⁰ − 2.5 R)/10³ (η₀ in µPa s, R per mass), λ_dil = 15/4 R η₀ (R in kJ/(kg K)), the reference's
/// residual term scaled by F_λ = √f h^(−2/3) √(M₀/M), and the Olchowy-Sengers term on the target with CoolProp's
/// defaults but the record's q_D (map 05 R2) and the fluid's own η, `eta`.
pub(crate) fn conductivity(
    ecs: &EcsConductivity,
    x: &TransportInputs<'_>,
    eta: &Result<ViscosityStages, Error>,
    references: &[Fluid],
) -> Result<ConductivityStages, Error> {
    let reference = reference(references, &ecs.reference)?;
    let psi = psi(&ecs.psi, ecs.psi_rho_r, x);
    // Equivalent (M8.8): T_r is 1 K in every v8.0.0 ECS block, so T/T_r is T·T_r.
    // equivalent mutant: replace / with *
    let f_int = sum(&ecs.f_int, x.t() / ecs.f_int_t_r);
    let eta_star = eta_dilute(ecs.lennard_jones.parameters(), x) * 1e6;
    let m = x.molar_mass();
    let r = x.gas_constant() / m;
    let cp0 = x.cp0() / m;
    let lambda_int = f_int * eta_star * (cp0 - 2.5 * r) / 1e3;
    let lambda_dilute = 15.0e-3 / 4.0 * (x.gas_constant() / (m * 1000.0)) * eta_star;
    let conformal = conformal_state(x, reference)?;
    let state = reference_state(reference, conformal, conformal.rho * psi)?;
    let transport = reference.transport()?;
    let own = transport.record.conductivity.as_ref().ok_or(Error::NoModel { prop: Prop::Conductivity })?;
    let ConductivityModel::Staged(staged) = &own.model else {
        return Err(LoadError::Format(
            format!("the ECS reference {} has no staged conductivity", ecs.reference).into(),
        )
        .into());
    };
    let inputs = TransportInputs::new(&state, reference.info().molar_mass(), reference.eos());
    let lambda_resid = residual(&staged.residual, &staged.reducing, &inputs);
    let (f, h) = (x.t() / conformal.t, conformal.rho / x.rho());
    let f_lambda = math::sqrt(f) * math::powf(h, -2.0 / 3.0) * math::sqrt(reference.info().molar_mass() / m);
    let critical = OlchowySengers { q_d: ecs.q_d, ..OlchowySengers::COOLPROP };
    let lambda_critical = olchowy_sengers(&critical, &ecs.reducing, x, eta)?;
    Ok(ConductivityStages {
        dilute: lambda_int + lambda_dilute,
        residual: lambda_resid * f_lambda,
        critical: lambda_critical,
    })
}

#[cfg(all(test, feature = "fluids-all"))]
mod tests {
    use super::*;

    /// A reference is found among several by its canonical name or an alias (the ECS blocks name n-Propane `Propane`);
    /// one not among them is a load error.
    #[test]
    fn a_reference_is_found_by_any_of_its_names() {
        let registry = crate::Registry::embedded().unwrap();
        let references = [registry.get("Nitrogen").unwrap().clone(), registry.get("n-Propane").unwrap().clone()];
        assert_eq!(reference(&references, "Propane").unwrap().info().name(), "n-Propane");
        assert_eq!(reference(&references, "Nitrogen").unwrap().info().name(), "Nitrogen");
        let missing = reference(&references, "R134a").unwrap_err();
        assert_eq!(missing.to_string(), "load error: bad data: the ECS reference fluid R134a is not loaded");
    }
}
