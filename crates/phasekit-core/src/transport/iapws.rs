//! The IAPWS transport releases CoolProp hard-codes (PLAN.md M8.7; map 05 U7): ordinary water's viscosity (R12-08,
//! Huber et al. 2009) and thermal conductivity (R15-11, Huber et al. 2012), whose critical terms read one correlation
//! length ξ, computed here once for both, and heavy water's (R4-84(2007)), closed forms of T and ρ. Each follows
//! CoolProp's operation order (`TransportRoutines.cpp:182-304, 865-943`), so the oracle agrees within
//! `TransportDirect`, with the releases' printed constants and two of CoolProp's (map 05 R12): π as 3.141592654 in
//! R15-11's Z(y), 1.3e-10 relative in λ₂, and R15-11's R = 0.46151805 kJ/(kg K) for c̄_p, 4.8e-14 from v8.0.0's R/M.
//! ξ reads (∂ρ/∂p)_T from the fluid's own EOS, as both releases specify (IAPWS-95 for Water).

use super::ConductivityStages;
use super::viscosity::{TransportInputs, ViscosityStages};
use crate::error::Error;
use crate::math;

/// R12-08's and R15-11's reference temperature T*, K.
const T_STAR: f64 = 647.096;
/// Their reference density ρ*, kg/m³.
const RHO_STAR: f64 = 322.0;
/// Their reference pressure p*, Pa.
const P_STAR: f64 = 22.064e6;
/// R15-11's specific gas constant R, J/(kg K) (Eq. (6)).
const R_WATER: f64 = 461.51805;
/// CoolProp's π in R15-11's Z(y) (`TransportRoutines.cpp:912`).
#[expect(clippy::approx_constant, reason = "CoolProp's truncated π, kept for oracle parity (map 05 R12)")]
const PI_COOLPROP: f64 = 3.141592654;

/// The critical-region constants R12-08 and R15-11 share (both Tables 3): ξ₀ in nm, Γ₀, ν, γ and T̄_R.
const XI_0: f64 = 0.13;
const GAMMA_0: f64 = 0.06;
const NU: f64 = 0.630;
const GAMMA: f64 = 1.239;
const T_BAR_R: f64 = 1.5;

/// R12-08's factors at one state: μ̄ = μ̄₀ μ̄₁ μ̄₂ in µPa s, and the correlation length ξ, nm, that μ̄₂ reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterViscosity {
    /// The dilute-gas factor μ̄₀(T̄) (Eq. (11)).
    pub mu0: f64,
    /// The residual factor μ̄₁(T̄, ρ̄) (Eq. (12)).
    pub mu1: f64,
    /// The critical enhancement μ̄₂ = exp(x_μ Y) (Eq. (14)).
    pub mu2: f64,
    /// ξ, nm (Eq. (21)).
    pub xi: f64,
}

impl WaterViscosity {
    /// μ̄₀ μ̄₁ μ̄₂, as CoolProp multiplies them, in µPa s.
    pub fn reduced(&self) -> f64 {
        self.mu0 * self.mu1 * self.mu2
    }

    /// The factors as additive stages, Pa s: η₀ = μ* μ̄₀, Δη = η₀ (μ̄₁ − 1), and the critical term η₀ μ̄₁ (μ̄₂ − 1).
    fn stages(&self) -> ViscosityStages {
        let dilute = self.mu0 * 1e-6;
        let residual = dilute * (self.mu1 - 1.0);
        let critical = dilute * self.mu1 * (self.mu2 - 1.0);
        ViscosityStages { dilute, initial_density: 0.0, residual, critical }
    }
}

/// R15-11's factors at one state: λ̄ = λ̄₀ λ̄₁ + λ̄₂ in mW/(m K).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterConductivity {
    /// The dilute-gas factor λ̄₀(T̄) (Eq. (16)).
    pub lambda0: f64,
    /// The residual factor λ̄₁(T̄, ρ̄) (Eq. (17)).
    pub lambda1: f64,
    /// The critical enhancement λ̄₂ (Eq. (18)).
    pub lambda2: f64,
}

impl WaterConductivity {
    /// λ̄₀ λ̄₁ + λ̄₂, as CoolProp sums them, in mW/(m K).
    pub fn reduced(&self) -> f64 {
        self.lambda0 * self.lambda1 + self.lambda2
    }

    /// The factors as additive stages, W/(m K): λ₀ = λ* λ̄₀, Δλ_r = λ₀ (λ̄₁ − 1), and Δλ_c = λ* λ̄₂.
    fn stages(&self) -> ConductivityStages {
        let dilute = self.lambda0 * 1e-3;
        ConductivityStages { dilute, residual: dilute * (self.lambda1 - 1.0), critical: self.lambda2 * 1e-3 }
    }
}

/// ξ, nm, at `x` (R12-08 Eqs. (21)-(23), R15-11 Eqs. (22)-(24)): ξ₀ (Δχ̄/Γ₀)^(ν/γ) with Δχ̄ = ρ̄ (ζ̄(T) − ζ̄(T_R) T̄_R/T̄)
/// and ζ̄ = (p*/ρ*) (∂ρ/∂p)_T from the EOS at the state's density; a negative Δχ̄ is set to zero (a NaN stays NaN).
fn correlation_length(x: &TransportInputs<'_>) -> f64 {
    let (t, rho_bar) = (x.t(), x.rho_mass() / RHO_STAR);
    // (∂ρ/∂p)_T in mass units: M over the molar (∂p/∂ρ)_T.
    let zeta = |t: f64| P_STAR / RHO_STAR * (x.molar_mass() / x.dp_drho_at(t));
    let delta_chi = rho_bar * (zeta(t) - zeta(T_BAR_R * T_STAR) * T_BAR_R / (t / T_STAR));
    let delta_chi = if delta_chi < 0.0 { 0.0 } else { delta_chi };
    XI_0 * math::powf(delta_chi / GAMMA_0, NU / GAMMA)
}

/// Σᵢ Σⱼ cᵢⱼ (1/T̄ − 1)ⁱ (ρ̄ − 1)ʲ over a table of rows i and columns j, in CoolProp's order.
fn double_sum<const J: usize>(c: &[[f64; J]], t_bar: f64, rho_bar: f64) -> f64 {
    let (u, v) = (1.0 / t_bar - 1.0, rho_bar - 1.0);
    let terms = (0..).zip(c).flat_map(|(i, row)| (0..).zip(row).map(move |(j, c)| (i, j, c)));
    terms.fold(0.0, |s, (i, j, c)| s + math::powi(u, i) * (c * math::powi(v, j)))
}

/// R12-08 Table 2: Hᵢⱼ of μ̄₁, rows i = 0..5, columns j = 0..6.
const H: [[f64; 7]; 6] = [
    [5.20094e-1, 2.22531e-1, -2.81378e-1, 1.61913e-1, -3.25372e-2, 0.0, 0.0],
    [8.50895e-2, 9.99115e-1, -9.06851e-1, 2.57399e-1, 0.0, 0.0, 0.0],
    [-1.08374, 1.88797, -7.72479e-1, 0.0, 0.0, 0.0, 0.0],
    [-2.89555e-1, 1.26613, -4.89837e-1, 0.0, 6.98452e-2, 0.0, -4.35673e-3],
    [0.0, 0.0, -2.57040e-1, 0.0, 0.0, 8.72102e-3, 0.0],
    [0.0, 1.20573e-1, 0.0, 0.0, 0.0, 0.0, -5.93264e-4],
];

/// Ordinary water's viscosity factors at `x` (IAPWS R12-08; `TransportRoutines.cpp:182-304`).
pub(crate) fn water_viscosity(x: &TransportInputs<'_>) -> WaterViscosity {
    let (t_bar, rho_bar) = (x.t() / T_STAR, x.rho_mass() / RHO_STAR);
    let mu0 = 100.0 * math::sqrt(t_bar)
        / (1.67752 + 2.20462 / t_bar + 0.6366564 / math::powi(t_bar, 2) - 0.241605 / math::powi(t_bar, 3));
    let mu1 = math::exp(rho_bar * double_sum(&H, t_bar, rho_bar));
    let xi = correlation_length(x);
    let mu2 = math::exp(0.068 * y_of_xi(xi));
    WaterViscosity { mu0, mu1, mu2, xi }
}

/// R12-08's Y(ξ) (Eqs. (15)-(20)), ξ in nm, with q_C⁻¹ = 1.9 nm and q_D⁻¹ = 1.1 nm: the series below ξ = 0.3817016416
/// nm, the closed form above.
fn y_of_xi(xi: f64) -> f64 {
    let (qc, qd) = (1.0 / 1.9, 1.0 / 1.1);
    let (c, d) = (qc * xi, qd * xi);
    if xi < 0.3817016416 {
        return 1.0 / 5.0 * c * math::powi(d, 5) * (1.0 - c + math::powi(c, 2) - 765.0 / 504.0 * math::powi(d, 2));
    }
    let psi = math::acos(math::powf(1.0 + math::powi(d, 2), -1.0 / 2.0));
    let w = math::sqrt(((c - 1.0) / (c + 1.0)).abs()) * math::tan(psi / 2.0);
    let l = if c > 1.0 { math::ln((1.0 + w) / (1.0 - w)) } else { 2.0 * math::atan(w.abs()) };
    1.0 / 12.0 * math::sin(3.0 * psi) - 1.0 / (4.0 * c) * math::sin(2.0 * psi)
        + 1.0 / math::powi(c, 2) * (1.0 - 5.0 / 4.0 * math::powi(c, 2)) * math::sin(psi)
        - 1.0 / math::powi(c, 3)
            * ((1.0 - 3.0 / 2.0 * math::powi(c, 2)) * psi - math::powf((math::powi(c, 2) - 1.0).abs(), 3.0 / 2.0) * l)
}

/// R15-11 Table 2: Lᵢⱼ of λ̄₁, rows i = 0..4, columns j = 0..5.
const L: [[f64; 6]; 5] = [
    [1.60397357, -0.646013523, 0.111443906, 0.102997357, -0.0504123634, 0.00609859258],
    [2.33771842, -2.78843778, 1.53616167, -0.463045512, 0.0832827019, -0.00719201245],
    [2.19650529, -4.54580785, 3.55777244, -1.40944978, 0.275418278, -0.0205938816],
    [-1.21051378, 1.60812989, -0.621178141, 0.0716373224, 0.0, 0.0],
    [-2.7203370, 4.57586331, -3.18369245, 1.1168348, -0.19268305, 0.012913842],
];

/// Ordinary water's conductivity factors at `x` (IAPWS R15-11; `TransportRoutines.cpp:882-943`); `mu` is the fluid's
/// own η there, Pa s, R12-08's with its critical factor (R15-11 Sec. 2.7).
pub(crate) fn water_conductivity(x: &TransportInputs<'_>, mu: f64) -> Result<WaterConductivity, Error> {
    let (t_bar, rho_bar) = (x.t() / T_STAR, x.rho_mass() / RHO_STAR);
    let lambda0 = math::sqrt(t_bar)
        / (2.443221e-3 + 1.323095e-2 / t_bar + 6.770357e-3 / math::powi(t_bar, 2) - 3.454586e-3 / math::powi(t_bar, 3)
            + 4.096266e-4 / math::powi(t_bar, 4));
    let lambda1 = math::exp(rho_bar * double_sum(&L, t_bar, rho_bar));
    let y = 1.0 / 0.40 * correlation_length(x);
    // Z(y) is 0 below 1.2e-7, where its closed form would cancel (Eq. (21)): no c_p, c_v or η is read there.
    if y < 1.2e-7 {
        return Ok(WaterConductivity { lambda0, lambda1, lambda2: 0.0 });
    }
    let (cp, cv) = (x.cp()?, x.cv()?);
    let kappa = cp / cv;
    let z = 2.0 / (PI_COOLPROP * y)
        * (((1.0 - 1.0 / kappa) * math::atan(y) + y / kappa)
            - (1.0 - math::exp(-1.0 / (1.0 / y + y * y / 3.0 / rho_bar / rho_bar))));
    let cp_bar = cp / x.molar_mass() / R_WATER;
    let lambda2 = 177.8514 * rho_bar * cp_bar * t_bar / (mu / 1e-6) * z;
    Ok(WaterConductivity { lambda0, lambda1, lambda2 })
}

/// Water's viscosity stages at `x`.
pub(crate) fn water_viscosity_stages(x: &TransportInputs<'_>) -> ViscosityStages {
    water_viscosity(x).stages()
}

/// Water's conductivity stages at `x`, with the fluid's own η stages.
pub(crate) fn water_conductivity_stages(
    x: &TransportInputs<'_>,
    eta: &Result<ViscosityStages, Error>,
) -> Result<ConductivityStages, Error> {
    let mu = eta.clone()?.total();
    water_conductivity(x, mu).map(|w| w.stages())
}

/// R4-84(2007)'s reference temperature T*, K, and density ρ*, kg/m³ (Eqs. (A1)-(A2), (B1)-(B2)).
const T_STAR_D2O: f64 = 643.847;
const RHO_STAR_D2O: f64 = 358.0;

/// R4-84(2007) Table A2: Bᵢⱼ of μ̄₁ with their exponents i and j, as CoolProp lists them.
const B_D2O: [(i32, i32, f64); 26] = [
    (0, 0, 0.4864192),
    (1, 0, -0.2448372),
    (2, 0, -0.8702035),
    (3, 0, 0.8716056),
    (4, 0, -1.051126),
    (5, 0, 0.3458395),
    (0, 1, 0.3509007),
    (1, 1, 1.315436),
    (2, 1, 1.297752),
    (3, 1, 1.353448),
    (0, 2, -0.2847572),
    (1, 2, -1.037026),
    (2, 2, -1.287846),
    (5, 2, -0.02148229),
    (0, 3, 0.07013759),
    (1, 3, 0.4660127),
    (2, 3, 0.2292075),
    (3, 3, -0.4857462),
    (0, 4, 0.01641220),
    (1, 4, -0.02884911),
    (3, 4, 0.1607171),
    (5, 4, -0.009603846),
    (0, 5, -0.01163815),
    (1, 5, -0.008239587),
    (5, 5, 0.004559914),
    (3, 6, -0.003886659),
];

/// Heavy water's viscosity stages at `x` (IAPWS R4-84(2007) Appendix A; `TransportRoutines.cpp:235-251`): η = μ* μ̄₀ μ̄₁
/// with μ* = 55.2651 µPa s, as η₀ = μ* μ̄₀ and Δη = η₀ (μ̄₁ − 1). The release has no critical term.
pub(crate) fn heavy_water_viscosity(x: &TransportInputs<'_>) -> ViscosityStages {
    let (t_bar, rho_bar) = (x.t() / T_STAR_D2O, x.rho_mass() / RHO_STAR_D2O);
    let a = [1.000000, 0.940695, 0.578377, -0.202044];
    let mu0 = math::sqrt(t_bar) / (a[0] + a[1] / t_bar + a[2] / math::powi(t_bar, 2) + a[3] / math::powi(t_bar, 3));
    let (u, v) = (1.0 / t_bar - 1.0, rho_bar - 1.0);
    let sum = B_D2O.iter().fold(0.0, |s, (i, j, b)| s + b * math::powi(u, *i) * math::powi(v, *j));
    let mu1 = math::exp(rho_bar * sum);
    let dilute = 55.2651e-6 * mu0;
    ViscosityStages { dilute, initial_density: 0.0, residual: dilute * (mu1 - 1.0), critical: 0.0 }
}

/// Heavy water's conductivity stages at `x` (IAPWS R4-84(2007) Appendix B; `TransportRoutines.cpp:865-880`):
/// λ = λ* (λ̄₀ + Δλ̄ + Δλ̄_c + Δλ̄_L) with λ* = 0.742128 mW/(m K), as λ₀ = λ* λ̄₀, Δλ_r = λ* (Δλ̄ + Δλ̄_L) and
/// Δλ_c = λ* Δλ̄_c.
pub(crate) fn heavy_water_conductivity(x: &TransportInputs<'_>) -> ConductivityStages {
    let (t_bar, rho_bar) = (x.t() / T_STAR_D2O, x.rho_mass() / RHO_STAR_D2O);
    let a = [1.00000, 37.3223, 22.5485, 13.0465, 0.0, -2.60735];
    let lambda0 = (1..).zip(&a[1..]).fold(a[0], |s, (i, a)| s + a * math::powi(t_bar, i));
    let (be, b) = (-2.506, [-167.310, 483.656, -191.039, 73.0358, -7.57467]);
    let background =
        (1..).zip(&b[1..]).fold(b[0] * (1.0 - math::exp(be * rho_bar)), |s, (j, b)| s + b * math::powi(rho_bar, j));
    let f1 = math::exp(0.144847 * t_bar + -5.64493 * math::powi(t_bar, 2));
    let f2 = math::exp(-2.80000 * math::powi(rho_bar - 1.0, 2))
        - 0.080738543 * math::exp(-17.9430 * math::powi(rho_bar - 0.125698, 2));
    let tau = t_bar / ((t_bar - 1.1).abs() + 1.1);
    let f3 = 1.0 + math::exp(60.0 * (tau - 1.0) + 20.0);
    let f4 = 1.0 + math::exp(100.0 * (tau - 1.0) + 15.0);
    let critical = 35429.6 * f1 * f2 * (1.0 + math::powi(f2, 2) * (5000.0e6 * math::powi(f1, 4) / f3 + 3.5 * f2 / f4));
    let liquid = -741.112 * math::powf(f1, 1.2) * (1.0 - math::exp(-math::powf(rho_bar / 2.5, 10.0)));
    let lambda_star = 0.742128e-3;
    ConductivityStages {
        dilute: lambda_star * lambda0,
        residual: lambda_star * (background + liquid),
        critical: lambda_star * critical,
    }
}
