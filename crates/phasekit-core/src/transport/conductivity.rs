//! Staged conductivity (PLAN.md M8.6; map 05 U4, R6, R10): λ = λ₀ + Δλ_r + Δλ_c, each stage a pure function of its
//! form's coefficients, of [`TransportInputs`] and of the fluid's own η, passed in explicitly (map 05 R6), in CoolProp's
//! operation order (`TransportRoutines.cpp:677-777, 850-863`), so the oracle agrees within `TransportDirect`. The
//! reducing state the forms read is the record's, snapshotted from the EOS at datagen (ROT-117), and so is what
//! Methane's hard-coded model reads (Friend, Ely and Ingham 1989; `TransportRoutines.cpp:1063-1140`). The other
//! hard-coded stages are `hardcoded`'s (M8.9).

use super::hardcoded;
use super::viscosity::{TransportInputs, ViscosityStages, sum};
use super::{
    ConductivityCritical, ConductivityDilute, ConductivityResidual, FriendMethane, HardcodedCriticalConductivity,
    HardcodedDiluteConductivity, OlchowySengers, Reducing, StagedConductivity,
};
use crate::error::Error;
use crate::math;

/// Boltzmann's constant as CoolProp's Olchowy-Sengers term has it (CODATA 2010; `CoolPropFluid.h:181`), J/K.
const BOLTZMANN: f64 = 1.3806488e-23;

/// The Olchowy-Sengers numerator below which there is no enhancement: zero, negative or rounding above zero (Lemmon and
/// Jacobsen 2004, p. 27; `TransportRoutines.cpp:760`).
// Equivalent (M8.6): the cut-off at ε/10 instead of 10ε changes only numerators below 2.2e-15, where the enhancement is
// about 1e-20 W/(m K).
// equivalent mutant: replace * with /
const NO_ENHANCEMENT: f64 = f64::EPSILON * 10.0;

/// The stages of a conductivity, W/(m K): CoolProp's `conductivity_contributions`, whose `initial_density` is 0 for
/// every staged fluid (`HelmholtzEOSMixtureBackend.cpp:922-1031`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConductivityStages {
    /// The dilute-gas term λ₀.
    pub dilute: f64,
    /// The residual term Δλ_r.
    pub residual: f64,
    /// The critical enhancement Δλ_c.
    pub critical: f64,
}

impl ConductivityStages {
    /// λ = λ₀ + Δλ_r + Δλ_c, summed as CoolProp sums them.
    pub fn total(&self) -> f64 {
        self.dilute + self.residual + self.critical
    }
}

/// The stages of `model` at `x`; `eta` is the fluid's own viscosity there, read only by the forms that need it.
pub(crate) fn stages(
    model: &StagedConductivity,
    x: &TransportInputs<'_>,
    eta: &Result<ViscosityStages, Error>,
) -> Result<ConductivityStages, Error> {
    let r = &model.reducing;
    let dilute = dilute(&model.dilute, r, x, eta)?;
    let residual = residual(&model.residual, r, x);
    let critical = match &model.critical {
        ConductivityCritical::OlchowySengers(os) => olchowy_sengers(os, r, x, eta)?,
        ConductivityCritical::Hardcoded(HardcodedCriticalConductivity::Absent) => 0.0,
        ConductivityCritical::Hardcoded(HardcodedCriticalConductivity::R123) => hardcoded::r123_critical(x, r),
        ConductivityCritical::Hardcoded(HardcodedCriticalConductivity::Ammonia) => hardcoded::ammonia_critical(x),
    };
    Ok(ConductivityStages { dilute, residual, critical })
}

/// The fluid's η at `x`, or why there is none.
fn viscosity(eta: &Result<ViscosityStages, Error>) -> Result<ViscosityStages, Error> {
    eta.clone()
}

/// λ₀, W/(m K) (`TransportRoutines.cpp:677-694, 850-863`).
fn dilute(
    form: &ConductivityDilute,
    r: &Reducing,
    x: &TransportInputs<'_>,
    eta: &Result<ViscosityStages, Error>,
) -> Result<f64, Error> {
    Ok(match form {
        ConductivityDilute::RatioOfPolynomials { numerator, denominator, t_r } => {
            let t_r = x.t() / t_r;
            sum(numerator, t_r) / sum(denominator, t_r)
        }
        ConductivityDilute::Eta0AndPoly(terms) => {
            // A₀ multiplies η₀ in µPa s; the other terms are powers of τ.
            let eta0 = viscosity(eta)?.dilute * 1e6;
            let tau = r.t / x.t();
            let (a0, a) = terms.a.split_first().unwrap_or((&0.0, &[]));
            let polynomial = a.iter().zip(terms.t.iter().skip(1));
            polynomial.fold(a0 * eta0, |s, (a, t)| s + a * math::powf(tau, *t))
        }
        ConductivityDilute::Hardcoded(HardcodedDiluteConductivity::CarbonDioxideHuberJpcrd2016) => {
            hardcoded::co2_dilute_conductivity(x, r)
        }
        ConductivityDilute::Hardcoded(HardcodedDiluteConductivity::Ethane) => {
            hardcoded::ethane_dilute_conductivity(x, r, viscosity(eta)?.dilute)
        }
    })
}

/// Δλ_r, W/(m K) (`TransportRoutines.cpp:696-725`).
pub(super) fn residual(form: &ConductivityResidual, r: &Reducing, x: &TransportInputs<'_>) -> f64 {
    let pw = math::powf;
    match form {
        ConductivityResidual::Polynomial { b, t, d, t_r, rho_r } => {
            let (tau, delta) = (t_r / x.t(), x.rho_mass() / rho_r);
            let terms = b.iter().zip(t.iter()).zip(d.iter());
            terms.fold(0.0, |s, ((b, t), d)| s + b * pw(tau, *t) * pw(delta, *d))
        }
        ConductivityResidual::PolynomialAndExponential { a, t, d, gamma, l } => {
            let (tau, delta) = (r.t / x.t(), x.rho() / r.rho);
            let terms = a.iter().zip(t.iter()).zip(d.iter()).zip(gamma.iter()).zip(l.iter());
            terms.fold(0.0, |s, ((((a, t), d), gamma), l)| {
                s + a * pw(tau, *t) * pw(delta, *d) * math::exp(-gamma * pw(delta, *l))
            })
        }
    }
}

/// The simplified Olchowy-Sengers crossover term, W/(m K) (`TransportRoutines.cpp:727-777`), with the fluid's own η.
pub(super) fn olchowy_sengers(
    os: &OlchowySengers,
    r: &Reducing,
    x: &TransportInputs<'_>,
    eta: &Result<ViscosityStages, Error>,
) -> Result<f64, Error> {
    // η first: a fluid whose viscosity a later step evaluates has no λ yet at any state, not only where the term is
    // non-zero (CoolProp reads η only past the cut-off below; the value is the same).
    let mu = viscosity(eta)?.total();
    let (t, rho) = (x.t(), x.rho());
    let t_ref = os.t_ref.unwrap_or(1.5 * r.t);
    let delta = rho / r.rho;
    let x_of = |dp_drho: f64| r.p / (r.rho * r.rho) * rho / dp_drho;
    let chi = x_of(x.dp_drho_at(t));
    let chi_ref = x_of(x.dp_drho_at(t_ref)) * t_ref / t;
    let num = chi - chi_ref;
    // Equivalent in practice (M8.6): CoolProp's strict comparison; `<=` differs only at a numerator of exactly 10ε,
    // which no state lands on. Either side is reached by the oracle's probes (`transport/probes.csv`).
    // equivalent mutant: replace < with <=
    if num < NO_ENHANCEMENT {
        return Ok(0.0);
    }
    let zeta = os.zeta0 * math::powf(num / os.big_gamma, os.nu / os.gamma);
    let (cp, cv) = (x.cp()?, x.cv()?);
    let pi = core::f64::consts::PI;
    let zq = zeta * os.q_d;
    let omega = 2.0 / pi * ((cp - cv) / cp * math::atan(zq) + cv / cp * zq);
    let omega0 = 2.0 / pi * (1.0 - math::exp(-1.0 / (1.0 / zq + 1.0 / 3.0 * zq * zq / delta / delta)));
    Ok(rho * cp * os.r0 * BOLTZMANN * t / (6.0 * pi * mu * zeta) * (omega - omega0))
}

/// Methane's whole conductivity, W/(m K): Friend, Ely and Ingham 1989 as CoolProp codes it, with its own viscosity, the
/// EOS's ideal-gas τ²α⁰_ττ and residual derivatives, and the record's critical point and saturated-vapour density.
pub(crate) fn friend_methane(m: &FriendMethane, x: &TransportInputs<'_>) -> Result<ConductivityStages, Error> {
    let (t, rho) = (x.t(), x.rho());
    let (delta, tau) = (rho / 10139.0, 190.55 / t);
    let pw = math::powf;
    let b = x.residual();
    let (delta_eos, tau_eos) = (rho / m.reducing.rho, m.reducing.t / t);
    // CoolProp multiplies the paper's δ and τ by the EOS's ∂α^r/∂δ and ∂²α^r/∂δ∂τ (map 05 R10).
    let (ar_d, ar_dd, ar_dt) = (b.a01 / delta_eos, b.a02 / (delta_eos * delta_eos), b.a11 / (delta_eos * tau_eos));
    // The viscosity of Friend 1989: dilute and residual.
    let c = [-3.0328138281, 16.918880086, -37.189364917, 41.288861858, -24.615921140, 8.9488430959, -1.8739245042];
    let c = [c[0], c[1], c[2], c[3], c[4], c[5], c[6], 0.20966101390, -9.6570437074e-3];
    let t_star = t / 174.0;
    let omega22 = c.iter().enumerate().fold(0.0, |s, (i, c)| s + c * pw(t_star, i as f64 / 3.0 - 1.0));
    let eta_dilute = 10.50 * math::sqrt(t_star) * omega22;
    let re = [1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 1.0, 1.0];
    let se = [0.0, 1.0, 0.0, 1.0, 1.5, 0.0, 2.0, 0.0, 1.0, 0.0, 1.0];
    let ge = [0.41250137, -0.14390912, 0.10366993, 0.40287464, -0.24903524, -0.12953131, 0.06575776, 0.02566628];
    let ge = [ge[0], ge[1], ge[2], ge[3], ge[4], ge[5], ge[6], ge[7], -0.03716526, -0.38798341, 0.03533815];
    let terms = ge.iter().zip(&re).zip(&se).map(|((g, r), s)| g * pw(delta, *r) * pw(tau, *s));
    let summer1 = terms.clone().take(9).fold(0.0, |s, term| s + term);
    let summer2 = terms.skip(9).fold(0.0, |s, term| s + term);
    let eta = 12.149 * summer1 / (1.0 + summer2) + eta_dilute;
    // λ₀, mW/(m K).
    let f_int = 1.458850 - 0.4377162 / t_star;
    let lambda_dilute = 0.51828 * eta_dilute * (3.75 - f_int * (x.ideal_tau_tau() + 1.5));
    // Δλ_r: Friend's δ²/δ_σ* term reads the saturated vapour's density below the critical point.
    let rl = [1.0, 3.0, 4.0, 4.0, 5.0, 5.0];
    let sl = [0.0, 0.0, 0.0, 1.0, 0.0, 1.0];
    let jl = [2.4149207, 0.55166331, -0.52837734, 0.073809553, 0.24465507, -0.047613626];
    let summer = jl.iter().zip(&rl).zip(&sl).fold(0.0, |s, ((j, r), l)| s + j * pw(delta, *r) * pw(tau, *l));
    // Equivalent in practice (M8.6): CoolProp's strict comparisons; `<=` differs only at T = T_c or ρ = ρ_c, where both
    // branches give the same λ to rounding. Either side is reached by the oracle's probes.
    // equivalent mutant: replace < with <=
    let delta_sigma = if t < m.t_c && rho < m.rho_c { m.rho_v.at(t) / m.rho_c } else { 1.0 };
    let lambda_residual = 6.29638 * (summer + 1.5554612 * (delta * delta) / delta_sigma);
    // Δλ_c.
    let (t_rel, rho_rel) = (1.0 - 1.0 / tau, 1.0 - delta);
    let f = math::exp(-2.646 * math::sqrt(t_rel.abs()) - 2.678 * (rho_rel * rho_rel) - -0.637 * rho_rel);
    let chi_19a = || 0.28631 * delta * tau / (1.0 + 2.0 * delta * ar_d + delta * delta * ar_dd);
    // Equivalent in practice (M8.6): CoolProp's strict comparison; `<=` differs only at |T*| = 0.03, which no state
    // lands on. Either side is reached by the oracle's probes.
    // equivalent mutant: replace < with <=
    let chi = if t_rel.abs() < 0.03 { friend_chi(t_rel, rho_rel).unwrap_or_else(chi_19a) } else { chi_19a() };
    let bracket = 1.0 + delta * ar_d - delta * tau * ar_dt;
    let lambda_critical = 91.855 / (eta * (tau * tau)) * (bracket * bracket) * pw(chi, 0.4681) * f;
    Ok(ConductivityStages {
        dilute: lambda_dilute * 0.001,
        residual: lambda_residual * 0.001,
        critical: lambda_critical * 0.001,
    })
}

/// Friend's χ_T* near the critical point (|T*| < 0.03): Eq. 26 on the critical isochore, Eq. 23 within |ρ*| < 0.03;
/// `None` elsewhere, where Eq. 19a applies.
fn friend_chi(t_rel: f64, rho_rel: f64) -> Option<f64> {
    let pw = math::powf;
    // Equivalent in practice (M8.6): CoolProp's strict comparison; `<=` differs only at |ρ*| = 1e-16, which no state
    // lands on. Either side is reached by the oracle's probes.
    // equivalent mutant: replace < with <=
    if rho_rel.abs() < 1e-16 {
        return Some(0.0801 * pw(t_rel.abs(), -1.190));
    }
    if rho_rel.abs() >= 0.03 {
        return None;
    }
    let (beta, w, s, e, a, b, r, q) = (0.355, -1.401, -6.098, 0.287, 3.352, 0.732, 0.535, 0.1133);
    let scaled = pw(rho_rel.abs(), -1.0 / beta);
    let omega = w * t_rel * scaled;
    // Equivalent (M8.6): this θ test, T* < −|ρ*|^(−1/β)/s, holds at every state that reaches it, since |ρ*| < 0.03 puts
    // the bound above 3000, and so does the mutant's −|ρ*|^(−1/β)·s; and `<=` differs only at equality, which no state
    // lands on.
    // equivalent mutant: replace < with <=
    // equivalent mutant: replace / with *
    let theta = if t_rel < -scaled / s { 1.0 + e * pw(1.0 + s * t_rel * scaled, 2.0 * beta) } else { 1.0 };
    Some(q * pw(rho_rel.abs(), -a) * pw(theta, b) / (theta + omega * (theta + r)))
}
