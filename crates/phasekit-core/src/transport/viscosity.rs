//! Staged viscosity (PLAN.md M8.5; map 05 U1, U3, §3.1): each stage a pure function of its form's coefficients and of
//! [`TransportInputs`], in CoolProp's operation order (`TransportRoutines.cpp:9-180, 358-401`), so the oracle agrees
//! within `TransportDirect`. The hardcoded stages (map 05 U10) and the Chung estimate of missing Lennard-Jones
//! parameters (map 05 R17) land at M8.9.

use super::{
    BatschinskiHildebrand, FrictionSecond, FrictionTheory, Kappa, LennardJones, Series, StagedViscosity,
    ViscosityDilute, ViscosityHigher, ViscosityInitial,
};
use crate::error::{Error, LoadError};
use crate::math;
use crate::prop::{DerivVar, Partial};
use crate::state::State;
use crate::units::Basis;

/// Avogadro's number as CoolProp's Rainwater-Friend term has it (CODATA 2010; `TransportRoutines.cpp:153`).
const AVOGADRO: f64 = 6.02214129e23;

/// What a transport form reads of a state (map 05 U1): T, the molar density, the molar mass and R, and, only when a form
/// needs them, the EOS's p and (∂p/∂T)_ρ at (T, ρ): a state's p is the EOS's at its (T, ρ), never a flash input
/// (map 05 R14).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TransportInputs<'a> {
    state: &'a State,
    molar_mass: f64,
}

impl<'a> TransportInputs<'a> {
    /// The inputs at `state` of a fluid of molar mass `molar_mass`, kg/mol.
    pub(crate) fn new(state: &'a State, molar_mass: f64) -> Self {
        TransportInputs { state, molar_mass }
    }

    fn t(&self) -> f64 {
        self.state.t()
    }

    fn rho(&self) -> f64 {
        self.state.rho(Basis::Molar)
    }

    fn dp_dt(&self) -> Result<f64, Error> {
        self.state.partial(Partial { of: DerivVar::P, wrt: DerivVar::T, at: DerivVar::Dmolar })
    }
}

/// The stages of a staged viscosity, Pa s: CoolProp's `viscosity_contributions`, whose `critical` is 0 for every staged
/// fluid (`HelmholtzEOSMixtureBackend.cpp:916-917`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViscosityStages {
    /// The dilute-gas term η₀.
    pub dilute: f64,
    /// The initial-density term η₁ (0 without one).
    pub initial_density: f64,
    /// The higher-order term Δη.
    pub residual: f64,
}

impl ViscosityStages {
    /// η = η₀ + η₁ + Δη, summed as CoolProp sums them.
    pub fn total(&self) -> f64 {
        self.dilute + self.initial_density + self.residual
    }
}

/// The typed "not yet" error of a form a later step evaluates.
pub(crate) fn lands(what: &str, step: &str) -> Error {
    LoadError::Format(format!("{what} lands at {step}").into()).into()
}

fn lennard_jones(lj: Option<LennardJones>) -> Result<LennardJones, Error> {
    lj.ok_or_else(|| lands("the Chung estimate of missing Lennard-Jones parameters", "M8.9"))
}

/// Σ aᵢ xᵗⁱ in the series' order.
fn sum(terms: &Series, x: f64) -> f64 {
    terms.a.iter().zip(&terms.t).fold(0.0, |s, (a, t)| s + a * math::powf(x, *t))
}

/// The stages of `model` at `x`; `lj` are the viscosity block's Lennard-Jones parameters.
pub(crate) fn stages(
    model: &StagedViscosity,
    lj: Option<LennardJones>,
    x: &TransportInputs<'_>,
) -> Result<ViscosityStages, Error> {
    let dilute = dilute(&model.dilute, lj, x)?;
    let initial_density = match &model.initial {
        Some(initial) => initial_density(initial, lj, dilute, x)?,
        None => 0.0,
    };
    let residual = higher(&model.higher, x)?;
    Ok(ViscosityStages { dilute, initial_density, residual })
}

/// η₀, Pa s (`TransportRoutines.cpp:9-102`).
fn dilute(form: &ViscosityDilute, lj: Option<LennardJones>, x: &TransportInputs<'_>) -> Result<f64, Error> {
    let t = x.t();
    Ok(match form {
        ViscosityDilute::CollisionIntegral { terms, molar_mass, c } => {
            let lj = lennard_jones(lj)?;
            let (t_star, sigma_nm, m) = (t / lj.epsilon_over_k, lj.sigma * 1e9, molar_mass * 1000.0);
            let s = math::exp(sum(terms, math::ln(t_star)));
            c * math::sqrt(m * t) / (sigma_nm * sigma_nm * s)
        }
        ViscosityDilute::KineticTheory => {
            let lj = lennard_jones(lj)?;
            let (t_star, sigma_nm, m) = (t / lj.epsilon_over_k, lj.sigma * 1e9, x.molar_mass * 1000.0);
            let omega22 = 1.16145 * math::powf(t_star, -0.14874)
                + 0.52487 * math::exp(-0.77320 * t_star)
                + 2.16178 * math::exp(-2.43787 * t_star);
            26.692e-9 * math::sqrt(m * t) / (sigma_nm * sigma_nm * omega22)
        }
        ViscosityDilute::PowersOfT(terms) => sum(terms, t),
        ViscosityDilute::PowersOfTr { terms, t_r } => sum(terms, t / t_r),
        ViscosityDilute::CollisionIntegralPowersOfTstar { terms, t_r, c } => c * math::sqrt(t) / sum(terms, t / t_r),
        ViscosityDilute::Hardcoded(h) => return Err(lands(&format!("the {} dilute viscosity term", h.name()), "M8.9")),
    })
}

/// η₁, Pa s (`TransportRoutines.cpp:140-180`; the host's product, `HelmholtzEOSMixtureBackend.cpp:758-762`).
fn initial_density(
    form: &ViscosityInitial,
    lj: Option<LennardJones>,
    dilute: f64,
    x: &TransportInputs<'_>,
) -> Result<f64, Error> {
    Ok(match form {
        ViscosityInitial::RainwaterFriend(terms) => {
            let lj = lennard_jones(lj)?;
            let b_star = sum(terms, x.t() / lj.epsilon_over_k);
            let b = AVOGADRO * math::powf(lj.sigma, 3.0) * b_star;
            dilute * b * x.rho()
        }
        ViscosityInitial::Empirical { n, d, t, t_r, rho_r } => {
            let (tau, delta) = (t_r / x.t(), x.rho() / rho_r);
            let terms = n.iter().zip(d.iter()).zip(t.iter());
            terms.fold(0.0, |s, ((n, d), t)| s + n * math::powf(delta, *d) * math::powf(tau, *t))
        }
    })
}

/// Δη, Pa s (`TransportRoutines.cpp:103-138, 358-401`).
fn higher(form: &ViscosityHigher, x: &TransportInputs<'_>) -> Result<f64, Error> {
    match form {
        ViscosityHigher::BatschinskiHildebrand(bh) => Ok(batschinski_hildebrand(bh, x)),
        ViscosityHigher::FrictionTheory(f) => friction_theory(f, x),
        ViscosityHigher::Hardcoded(h) => Err(lands(&format!("the {} higher-order viscosity term", h.name()), "M8.9")),
    }
}

fn batschinski_hildebrand(bh: &BatschinskiHildebrand, x: &TransportInputs<'_>) -> f64 {
    let (delta, tau) = (x.rho() / bh.rho_r, bh.t_r / x.t());
    let pw = math::powf;
    let first = bh.a.iter().zip(bh.d1.iter()).zip(bh.t1.iter()).zip(bh.gamma.iter()).zip(bh.l.iter());
    let s = first.fold(0.0, |s, ((((a, d1), t1), gamma), l)| {
        s + a * pw(delta, *d1) * pw(tau, *t1) * math::exp(gamma * pw(delta, *l))
    });
    let second = bh.f.iter().zip(bh.d2.iter()).zip(bh.t2.iter());
    let f = second.fold(0.0, |s, ((f, d2), t2)| s + f * pw(delta, *d2) * pw(tau, *t2));
    let numerator = bh.g.iter().zip(bh.h.iter()).fold(0.0, |s, (g, h)| s + g * pw(tau, *h));
    let denominator = bh.p.iter().zip(bh.q.iter()).fold(0.0, |s, (p, q)| s + p * pw(tau, *q));
    let delta0 = numerator / denominator;
    s + f * (1.0 / (delta0 - delta) - 1.0 / delta0)
}

/// One friction coefficient (A₀ + A₁ψ₁ + A₂ψ₂) τᴺ.
fn kappa(k: &Kappa, psi1: f64, psi2: f64, tau: f64) -> f64 {
    (k.a[0] + k.a[1] * psi1 + k.a[2] * psi2) * math::powf(tau, f64::from(k.n))
}

/// Friction theory, pressures in bar (Quiñones-Cisneros and Deiters 2006).
fn friction_theory(f: &FrictionTheory, x: &TransportInputs<'_>) -> Result<f64, Error> {
    let tau = f.t_r / x.t();
    let (psi1, psi2) = (math::exp(tau) - f.c1, math::exp(tau * tau) - f.c2);
    let ki = (f.ki[0] + f.ki[1] * psi1 + f.ki[2] * psi2) * tau;
    let (ka, kr, kaa) = (kappa(&f.ka, psi1, psi2, tau), kappa(&f.kr, psi1, psi2, tau), kappa(&f.kaa, psi1, psi2, tau));
    let (krr, kdrdr) = match &f.second {
        FrictionSecond::Rr(k) => (kappa(k, psi1, psi2, tau), 0.0),
        FrictionSecond::Drdr(k) => (0.0, kappa(k, psi1, psi2, tau)),
    };
    let kii = f.kii.as_ref().map_or(0.0, |k| kappa(k, psi1, psi2, tau));
    let (krrr, kaaa) =
        f.cubic.as_ref().map_or((0.0, 0.0), |c| (kappa(&c.krrr, psi1, psi2, tau), kappa(&c.kaaa, psi1, psi2, tau)));
    let p = x.state.p() / 1e5;
    let pr = x.t() * x.dp_dt()? / 1e5;
    let pa = p - pr;
    let pid = x.rho() * x.state.gas_constant() * x.t() / 1e5;
    let dpr = pr - pid;
    Ok(ka * pa
        + kr * dpr
        + ki * pid
        + kaa * pa * pa
        + kdrdr * dpr * dpr
        + krr * pr * pr
        + kii * pid * pid
        + krrr * pr * pr * pr
        + kaaa * pa * pa * pa)
}
