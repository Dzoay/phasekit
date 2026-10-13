//! CoolProp's hard-coded transport forms and its two generalised viscosity models (PLAN.md M8.9; map 05 U9, U10): each a
//! closed form of the state in CoolProp's operation order (`TransportRoutines.cpp:9-26, 305-356, 403-675, 779-782,
//! 830-848, 945-1061, 1266-1292`), so the oracle agrees within `TransportDirect`, with each correlation's constants as
//! CoolProp has them (map 05 R12: Ammonia's k_B and π). A whole model is split into stages that add up to CoolProp's
//! total: the dilute term is its zero-density limit, CoolProp files the whole under `critical` (map 05 R8).

use super::viscosity::{TransportInputs, ViscosityStages, kinetic_theory};
use super::{Chung, ConductivityStages, LennardJones, Reducing, RhoSrCs};
use crate::derivs::Order;
use crate::error::Error;
use crate::math;
use crate::prop::Prop;

/// x^n as CoolProp's `pow(x, n)` computes it, the C library's `pow`, not a multiplication chain: methanol's
/// hard-sphere term amplifies the difference past `TransportDirect` (Ammonia's `powInt` is `math::powi`).
fn pow_n(x: f64, n: i32) -> f64 {
    math::powf(x, f64::from(n))
}

/// Avogadro's number as CoolProp's methanol model has it (CODATA 2010).
const AVOGADRO: f64 = 6.02214129e23;

/// CO₂'s triple-point temperature, K: Laesecke and Muzny's T_t and v8.0.0's EOS `Ttriple`, which CoolProp's higher-order
/// term reads (`TransportRoutines.cpp:349`).
pub(crate) const CO2_T_TRIPLE: f64 = 216.592;

/// Helium's viscosity, Arp, McCarty and Friend 1998 as REFPROP codes it (`TransportRoutines.cpp:403-438`), ρ in g/cm³:
/// the dilute term exp(η̄₀), or above 100 K the separate η₀, and the residual exp(η̄₀ + η̄_E) − exp(η̄₀).
pub(crate) fn helium(x: &TransportInputs<'_>) -> ViscosityStages {
    let (rho, t) = (x.rho_mass() / 1000.0, x.t());
    let l = if t <= 300.0 { math::ln(t) } else { math::ln(300.0) };
    let b = -47.5295259 / l + 87.6799309 - 42.0741589 * l + 8.33128289 * l * l - 0.589252385 * l * l * l;
    let c = 547.309267 / l - 904.870586 + 431.404928 * l - 81.4504854 * l * l + 5.37008433 * l * l * l;
    let d = -1684.39324 / l + 3331.08630 - 1632.19172 * l + 308.804413 * l * l - 20.2936367 * l * l * l;
    let eta_0_slash = -0.135311743 / l + 1.00347841 + 1.20654649 * l - 0.149564551 * l * l + 0.012520841 * l * l * l;
    let eta_e_slash = rho * b + rho * rho * c + rho * rho * rho * d;
    let ln_eta = eta_0_slash + eta_e_slash;
    let dilute = if t <= 100.0 {
        math::exp(eta_0_slash)
    } else {
        196.0 * math::powf(t, 0.71938) * math::exp(12.451 / t - 295.67 / t / t - 4.1249)
    };
    // µg/(cm s) to Pa s.
    let residual = math::exp(ln_eta) - math::exp(eta_0_slash);
    ViscosityStages {
        dilute: dilute / 10.0 / 1e6,
        initial_density: 0.0,
        residual: residual / 10.0 / 1e6,
        critical: 0.0,
    }
}

/// Methanol's viscosity, Xiang, Laesecke and Huber 2006 (`TransportRoutines.cpp:440-496`): η₀ (f η_g + (1 − f) η_E),
/// the Rainwater-Friend gas and the hard-sphere liquid blended by f = 1/(1 + exp(5 (ρ_r − 1))); the dilute term is η₀.
pub(crate) fn methanol(x: &TransportInputs<'_>) -> ViscosityStages {
    let (epsilon_over_k, sigma0, delta, m, t) = (577.87, 0.3408e-9, 0.4575, 32.04216, x.t());
    let rhomolar = x.rho();
    let t_star = t / epsilon_over_k;
    let (rhor, tr) = (x.rho_mass() / 273.0, t / 512.6);
    let b = [-19.572881, 219.73999, -1015.3226, 2471.01251, -3375.1717, 2491.6597, -787.26086, 14.085455, -0.34664158];
    let te = [0.0, -0.25, -0.5, -0.75, -1.0, -1.25, -1.5, -2.5, -5.5];
    let b_eta_star = b.iter().zip(te).fold(0.0, |s, (b, t)| s + b * math::powf(t_star, t));
    let b_eta = AVOGADRO * pow_n(sigma0, 3) * b_eta_star;
    let c = [1.86222085e-3, 9.990338];
    let c_eta_star = c[0] * pow_n(t_star, 3) * math::exp(c[1] * math::powf(t_star, -0.5));
    let c_eta = pow_n(AVOGADRO * pow_n(sigma0, 3), 2) * c_eta_star;
    let eta_g = 1.0 + b_eta * rhomolar + c_eta * rhomolar * rhomolar;
    let a = [1.16145, -0.14874, 0.52487, -0.77320, 2.16178, -2.43787, 0.95976e-3, 0.10225, -0.97346, 0.10657, -0.34528];
    let a = [a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8], a[9], a[10], -0.44557, -2.58055];
    let d = [-1.181909, 0.5031030, -0.6268461, 0.5169312, -0.2351349, 5.3980235e-2, -4.9069617e-3];
    let e = [4.018368, -4.239180, 2.245110, -0.5750698, 2.3021026e-2, 2.5696775e-2, -6.8372749e-3, 7.2707189e-4];
    let e = [e[0], e[1], e[2], e[3], e[4], e[5], e[6], e[7], -2.9255711e-5];
    let omega_lj = a[0] * math::powf(t_star, a[1]) + a[2] * math::exp(a[3] * t_star) + a[4] * math::exp(a[5] * t_star);
    let omega_delta =
        a[7] * math::powf(t_star, a[8]) + a[9] * math::exp(a[10] * t_star) + a[11] * math::exp(a[12] * t_star);
    let omega_sm = omega_lj * (1.0 + pow_n(delta, 2) / (1.0 + a[6] * pow_n(delta, 6)) * omega_delta);
    let eta_0 = 2.66957e-26 * math::sqrt(m * t) / (pow_n(sigma0, 2) * omega_sm);
    let summer = (0..).zip(d).fold(0.0, |s, (i, d)| s + d / pow_n(tr, i));
    let summer = (1..).zip(e).fold(summer, |s, (j, e)| s + e * pow_n(rhor, j));
    let sigma_hs = summer * 0.7193422e-9;
    let b = 2.0 * core::f64::consts::PI * AVOGADRO * pow_n(sigma_hs, 3) / 3.0;
    let zeta = b * rhomolar / 4.0;
    let g = (1.0 - 0.5 * zeta) / pow_n(1.0 - zeta, 3);
    let eta_e = 1.0 / g + 0.8 * b * rhomolar + 0.761 * g * pow_n(b * rhomolar, 2);
    let f = 1.0 / (1.0 + math::exp(5.0 * (rhor - 1.0)));
    let total = eta_0 * (f * eta_g + (1.0 - f) * eta_e);
    ViscosityStages { dilute: eta_0, initial_density: 0.0, residual: total - eta_0, critical: 0.0 }
}

/// The (dilute-gas, liquid-like, critical) blend Shan, Penoncello and Jacobsen's R23 models share, µPa s or mW/(m K):
/// ((ρ_L − ρ̄)/ρ_L)^c X_DG + (ρ̄/ρ_L)^c X_L + Δ_c, as stages (X_DG, the blend's rest, Δ_c). `liquid` is (C₂, ΔG*/R_u,
/// ρ_L): their ΔG* over their R_u, 8.31451 J/(mol K), one correlation constant, since no gas-constant literal enters
/// the kernel (ROT-042).
fn shan_blend(x: &TransportInputs<'_>, dilute: f64, liquid: (f64, f64, f64), c: f64, max: f64) -> [f64; 3] {
    let (c2, delta_g_over_r, rho_l) = liquid;
    let (rhocbar, tc, t) = (7.5114, 299.2793, x.t());
    let rhobar = x.rho() / 1000.0;
    let liquid = c2 * (rho_l * rho_l) / (rho_l - rhobar)
        * math::sqrt(t)
        * math::exp(rhobar / (rho_l - rhobar) * delta_g_over_r / t);
    let (chi, tau) = (rhobar - rhocbar, t - tc);
    let critical = 4.0 * max / ((math::exp(chi) + math::exp(-chi)) * (math::exp(tau) + math::exp(-tau)));
    let blend = math::powf((rho_l - rhobar) / rho_l, c) * dilute + math::powf(rhobar / rho_l, c) * liquid;
    [dilute, blend - dilute, critical]
}

/// R23's viscosity, Shan, Penoncello and Jacobsen 2000 (`TransportRoutines.cpp:498-518`).
pub(crate) fn r23(x: &TransportInputs<'_>) -> ViscosityStages {
    let (molar_mass, e_k, sigma) = (70.014, 243.91, 0.4278);
    let a = [0.4425728, -0.5138403, 0.1547566, -0.02821844, 0.001578286];
    let ln_t = math::ln(x.t() / e_k);
    let omega = math::exp(a[0] + a[1] * ln_t + a[2] * pow_n(ln_t, 2) + a[3] * pow_n(ln_t, 3) + a[4] * pow_n(ln_t, 4));
    let eta_dg = 1.25 * 0.021357 * math::sqrt(molar_mass * x.t()) / (sigma * sigma * omega);
    // ΔG* = 771.23 J/mol.
    let [dilute, residual, critical] = shan_blend(x, eta_dg, (0.1832, 92.75711978216395, 32.174), 1.3163, 3.967);
    ViscosityStages { dilute: dilute / 1e6, initial_density: 0.0, residual: residual / 1e6, critical: critical / 1e6 }
}

/// The xylenes' common dilute and initial-density terms (Cao et al. 2016; Balogun et al. 2016), µPa s: 0.22 √T/S_η and
/// (A₁ + B₁/T + C₁/T²) ρ, ρ in mol/L, with `c0` the fluid's dilute prefactor.
fn xylene_dilute(x: &TransportInputs<'_>, c0: f64) -> (f64, f64) {
    let t = x.t();
    let ln_s_eta = -1.4933 + 473.2 / t + -57033.0 / (t * t);
    let eta0 = c0 * math::sqrt(t) / math::exp(ln_s_eta);
    let eta1 = (13.2814 + -10862.4 / t + 1664060.0 / (t * t)) * (x.rho() / 1000.0);
    (eta0, eta1)
}

/// o- and m-xylene's residual term (Cao et al. 2016), µPa s, from its constants.
fn cao_residual(
    x: &TransportInputs<'_>,
    (t_c, rho_c): (f64, f64),
    d: [f64; 5],
    n: [f64; 5],
    e: [f64; 5],
    k: [f64; 3],
) -> f64 {
    let (tr, rhor) = (x.t() / t_c, x.rho() / 1000.0 / rho_c);
    let pw = math::powf;
    let f = (d[0] + e[0] * pw(tr, -k[0])) * pw(rhor, n[0])
        + d[1] * pw(rhor, n[1])
        + e[2] * pw(rhor, n[2]) / pw(tr, k[2])
        + (d[3] * rhor + e[3] * tr) * pw(rhor, n[3])
        + d[4] * pw(rhor, n[4]);
    pw(rhor, 2.0 / 3.0) * math::sqrt(tr) * f
}

/// The stages of a xylene's three terms, µPa s.
fn xylene_stages(eta0: f64, eta1: f64, residual: f64) -> ViscosityStages {
    ViscosityStages { dilute: eta0 / 1e6, initial_density: eta1 / 1e6, residual: residual / 1e6, critical: 0.0 }
}

/// o-xylene's viscosity, Cao et al. 2016 (`TransportRoutines.cpp:520-540`).
pub(crate) fn o_xylene(x: &TransportInputs<'_>) -> ViscosityStages {
    let (eta0, eta1) = xylene_dilute(x, 0.22225);
    let d = [-2.05581e-3, 2.38762, 0.0, 10.4497, 15.9587];
    let (n, e, k) = ([10.3, 3.3, 25.0, 0.7, 0.4], [2.65651e-3, 0.0, 1.77616e-12, -18.2446, 0.0], [0.8, 0.0, 4.4]);
    xylene_stages(eta0, eta1, cao_residual(x, (630.259, 2.6845), d, n, e, k))
}

/// m-xylene's viscosity, Cao et al. 2016 (`TransportRoutines.cpp:541-561`).
pub(crate) fn m_xylene(x: &TransportInputs<'_>) -> ViscosityStages {
    let (eta0, eta1) = xylene_dilute(x, 0.22115);
    let d = [-0.268950, -0.0290018, 0.0, 14.7728, 17.1128];
    let (n, e, k) = ([6.8, 3.3, 22.0, 0.6, 0.4], [0.320971, 0.0, 1.72866e-10, -18.9852, 0.0], [0.3, 0.0, 3.2]);
    xylene_stages(eta0, eta1, cao_residual(x, (616.89, 2.665), d, n, e, k))
}

/// p-xylene's viscosity, Balogun et al. 2016 (`TransportRoutines.cpp:562-579`).
pub(crate) fn p_xylene(x: &TransportInputs<'_>) -> ViscosityStages {
    let (eta0, eta1) = xylene_dilute(x, 0.22005);
    let (tr, rhor) = (x.t() / 616.168, x.rho() / 1000.0 / 2.69392);
    let (pw, pi) = (math::powf, pow_n);
    let sum1 = 122.919 * pw(rhor, 1.5) - 282.329 * pi(rhor, 2) + 279.348 * pi(rhor, 3) - 146.776 * pi(rhor, 4)
        + 28.361 * pi(rhor, 5)
        - 0.004585 * pi(rhor, 11);
    let sum2 = 15.337 * pw(rhor, 1.5) - 0.0004382 * pi(rhor, 11) + 0.00002307 * pi(rhor, 15);
    let residual = pw(rhor, 2.0 / 3.0) * (sum1 + 1.0 / math::sqrt(tr) * sum2);
    xylene_stages(eta0, eta1, residual)
}

/// Ethane's dilute viscosity, Friend, Ingham and Ely 1991 (`TransportRoutines.cpp:581-592`), Pa s.
pub(crate) fn ethane_dilute(t: f64) -> f64 {
    let c = [-3.0328138281, 16.918880086, -37.189364917, 41.288861858, -24.615921140, 8.9488430959, -1.8739245042];
    let c = [c[0], c[1], c[2], c[3], c[4], c[5], c[6], 0.20966101390, -9.6570437074e-3];
    let t_star = t / 245.0;
    let omega = (0..).zip(c).fold(0.0, |s, (i, c)| s + c * math::powf(t_star, f64::from(i) / 3.0 - 1.0));
    12.0085 * math::sqrt(t_star) * omega / 1e6
}

/// Cyclohexane's dilute viscosity, Tariq et al. 2014 (`TransportRoutines.cpp:593-598`), Pa s.
pub(crate) fn cyclohexane_dilute(t: f64) -> f64 {
    let s_eta = math::exp(-1.5093 + 364.87 / t - 39537.0 / pow_n(t, 2));
    0.19592 * math::sqrt(t) / s_eta / 1e6
}

/// CO₂'s dilute viscosity, Laesecke and Muzny 2017 Eq. (4) (`TransportRoutines.cpp:600-612`), Pa s.
pub(crate) fn co2_dilute(t: f64) -> f64 {
    let a = [1749.35489318835, -369.069300007128, 5423856.34887691, -2.21283852168356, -269503.247933569];
    let a = [a[0], a[1], a[2], a[3], a[4], 73145.021531826, 5.34368649509278];
    let (t6, t3) = (math::powf(t, 1.0 / 6.0), math::powf(t, 1.0 / 3.0));
    let den =
        a[0] + a[1] * t6 + a[2] * math::exp(a[3] * t3) + (a[4] + a[5] * t3) / math::exp(t3) + a[6] * math::sqrt(t);
    0.0010055 * math::sqrt(t) / den
}

/// Toluene's higher-order viscosity, Avgeri et al. 2015 (`TransportRoutines.cpp:305-310`), Pa s.
pub(crate) fn toluene_higher(x: &TransportInputs<'_>) -> f64 {
    let (tr, rhor) = (x.t() / 591.75, x.rho_mass() / 291.987);
    let c = [19.919216, -2.6557905, -135.904211, -7.9962719, -11.014795, -10.113817];
    1e-6 * math::powf(rhor, 2.0 / 3.0)
        * math::sqrt(tr)
        * ((c[0] * rhor + c[1] * pow_n(rhor, 4)) / tr
            + c[2] * rhor * rhor * rhor / (rhor * rhor + c[3] + c[4] * tr)
            + c[5] * rhor)
}

/// Hydrogen's higher-order viscosity, Muzny et al. 2013 (`TransportRoutines.cpp:312-316`), Pa s; para-hydrogen's model
/// reads it too, with normal hydrogen's constants.
pub(crate) fn hydrogen_higher(x: &TransportInputs<'_>) -> f64 {
    let (tr, rhor) = (x.t() / 33.145, x.rho_mass() * 0.011);
    let c = [6.43449673e-6, 4.56334068e-2, 2.32797868e-1, 9.58326120e-1, 1.27941189e-1, 3.63576595e-1];
    let exponent = c[1] * tr + c[2] / tr + c[3] * pow_n(rhor, 2) / (c[4] + tr) + c[5] * pow_n(rhor, 6);
    c[0] * pow_n(rhor, 2) * math::exp(exponent)
}

/// Benzene's higher-order viscosity, Avgeri et al. 2014 (`TransportRoutines.cpp:317-323`), Pa s.
pub(crate) fn benzene_higher(x: &TransportInputs<'_>) -> f64 {
    let (tr, rhor) = (x.t() / 562.02, x.rho_mass() / 304.792);
    let c = [-9.98945, 86.06260, 2.74872, 1.11130, -1.0, -134.1330, -352.473, 6.60989, 88.4174];
    1e-6 * math::powf(rhor, 2.0 / 3.0)
        * math::sqrt(tr)
        * (c[0] * pow_n(rhor, 2)
            + c[1] * rhor / (c[2] + c[3] * tr + c[4] * rhor)
            + (c[5] * rhor + c[6] * pow_n(rhor, 2)) / (c[7] + c[8] * pow_n(rhor, 2)))
}

/// n-hexane's higher-order viscosity, Michailidou et al. 2013 (`TransportRoutines.cpp:324-334`), Pa s.
pub(crate) fn hexane_higher(x: &TransportInputs<'_>) -> f64 {
    let (tr, rhor) = (x.t() / 507.82, x.rho_mass() / 233.182);
    let c = [2.53402335 / 1e6, -9.724061002 / 1e6, 0.469437316, 158.5571631, 72.42916856 / 1e6, 10.60751253];
    let c = [c[0], c[1], c[2], c[3], c[4], c[5], 8.628373915, -6.61346441, -2.212724566];
    math::powf(rhor, 2.0 / 3.0)
        * math::sqrt(tr)
        * (c[0] / tr
            + c[1] / (c[2] + tr + c[3] * rhor * rhor)
            + c[4] * (1.0 + rhor) / (c[5] + c[6] * tr + c[7] * rhor + rhor * rhor + c[8] * rhor * tr))
}

/// n-heptane's higher-order viscosity, Michailidou et al. 2014 (`TransportRoutines.cpp:336-345`), Pa s.
pub(crate) fn heptane_higher(x: &TransportInputs<'_>) -> f64 {
    let (tr, rhor) = (x.t() / 540.13, x.rho_mass() / 232.0);
    let c = [22.15000 / 1e6, -15.00870 / 1e6, 3.71791 / 1e6, 77.72818 / 1e6, 9.73449, 9.51900, -6.34076, -2.51909];
    math::powf(rhor, 2.0 / 3.0)
        * math::sqrt(tr)
        * (c[0] * rhor
            + c[1] * pow_n(rhor, 2)
            + c[2] * pow_n(rhor, 3)
            + c[3] * rhor / (c[4] + c[5] * tr + c[6] * rhor + rhor * rhor + c[7] * rhor * tr))
}

/// CO₂'s higher-order viscosity, Laesecke and Muzny 2017 Eqs. (8)-(9) (`TransportRoutines.cpp:347-356`), Pa s, with the
/// EOS's R and M and CO₂'s triple point.
pub(crate) fn co2_higher(x: &TransportInputs<'_>) -> f64 {
    let (c1, c2, gamma) = (0.360603235428487, 0.121550806591497, 8.06282737481277);
    let (tt, rho_tl) = (CO2_T_TRIPLE, 1178.53);
    let (tr, rhor) = (x.t() / tt, x.rho_mass() / rho_tl);
    let eta_tl = math::powf(rho_tl, 2.0 / 3.0) * math::sqrt(x.gas_constant() * tt)
        / (math::powf(x.molar_mass(), 1.0 / 6.0) * 84446887.43579945);
    eta_tl * (c1 * tr * pow_n(rhor, 3) + (pow_n(rhor, 2) + math::powf(rhor, gamma)) / (tr - c2))
}

/// Ethane's higher-order viscosity, Friend, Ingham and Ely 1991 (`TransportRoutines.cpp:614-629`), Pa s.
pub(crate) fn ethane_higher(x: &TransportInputs<'_>) -> f64 {
    let r = [1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 1.0, 1.0];
    let s = [0.0, 1.0, 0.0, 1.0, 1.5, 0.0, 2.0, 0.0, 1.0, 0.0, 1.0];
    let g = [0.47177003, -0.23950311, 0.39808301, -0.27343335, 0.35192260, -0.21101308, -0.00478579, 0.07378129];
    let g = [g[0], g[1], g[2], g[3], g[4], g[5], g[6], g[7], -0.030435255, -0.30435286, 0.001215675];
    let (tau, delta) = (305.33 / x.t(), x.rho() / 6870.0);
    let term = |i: usize| g[i] * math::powf(delta, r[i]) * math::powf(tau, s[i]);
    let sum1 = (0..9).fold(0.0, |s, i| s + term(i));
    let sum2 = (9..11).fold(0.0, |s, i| s + term(i));
    15.977 * sum1 / (1.0 + sum2) / 1e6
}

/// ρ*_sr-CS viscosity, Bell 2016 (`TransportRoutines.cpp:1266-1292`): η = [1 + C (η*_ref − 1)] η₀ with η*_ref =
/// exp(ψ f_liq + (1 − ψ) f_vap), ψ = 1/(1 + exp(−100 (x − x_crossover))) and x = ρ s_r/(ρ s_r)_c, s_r/R = τα^r_τ − α^r.
/// CoolProp fixes the crossover at 2 and ignores the record's (map 05 R4); every v8.0.0 record stores 2, and this reads it.
pub(crate) fn rhosr(model: &RhoSrCs, lj: LennardJones, x: &TransportInputs<'_>) -> ViscosityStages {
    let eta_dilute = kinetic_theory(lj, x.molar_mass(), x.t());
    let a = x.eos().residual(x.t(), x.rho(), Order::One);
    let (a00, a10) = (a.get(0, 0).unwrap_or(f64::NAN), a.get(1, 0).unwrap_or(f64::NAN));
    let xs = x.rho() * x.gas_constant() * (a10 - a00) / model.rhosr_critical;
    let psi_liq = 1.0 / (1.0 + math::exp(-100.0 * (xs - model.x_crossover)));
    let (cl, cv) = (model.c_liq, model.c_vap);
    let f_liq = cl[0] + xs * (cl[1] + xs * (cl[2] + xs * cl[3]));
    let f_vap = cv[0] + xs * (cv[1] + xs * (cv[2] + xs * cv[3]));
    let etastar_ref = math::exp(psi_liq * f_liq + (1.0 - psi_liq) * f_vap);
    let total = (1.0 + model.c * (etastar_ref - 1.0)) * eta_dilute;
    ViscosityStages { dilute: eta_dilute, initial_density: 0.0, residual: total - eta_dilute, critical: 0.0 }
}

/// Chung et al. 1988 viscosity with its own block's constants (`TransportRoutines.cpp:630-675`): the dilute η₀ (Neufeld's
/// Ω₂₂ with its sine term) times 1/G₂ + A₆Y, and the dense term η_p.
pub(crate) fn chung(data: &Chung, x: &TransportInputs<'_>) -> ViscosityStages {
    let a0 = [6.32402, 0.12102e-2, 5.28346, 6.62263, 19.74540, -1.89992, 24.27450, 0.79716, -0.23816, 0.68629e-1];
    let a1 = [50.41190, -0.11536e-2, 254.20900, 38.09570, 7.63034, -12.53670, 3.44945, 1.11764, 0.67695e-1, 0.34793];
    let a2 =
        [-51.68010, -0.62571e-2, -168.48100, -8.46414, -14.35440, 4.98529, -11.29130, 0.12348e-1, -0.81630, 0.59256];
    let a3 = [1189.02000, 0.37283e-1, 3898.27000, 31.41780, 31.52670, -18.15070, 69.34660, -4.11661, 4.02528, -0.72663];
    let vc = 1.0 / (data.rho_c / 1e6);
    let (omega, m, tc, mu_d, kappa) =
        (data.acentric, data.molar_mass * 1000.0, data.t_c, data.dipole_moment, data.kappa);
    let mu_r = 131.3 * mu_d / math::sqrt(vc * tc);
    let a: [f64; 10] = core::array::from_fn(|i| a0[i] + a1[i] * omega + a2[i] * pow_n(mu_r, 4) + a3[i] * kappa);
    let f_c = 1.0 - 0.2756 * omega + 0.059035 * pow_n(mu_r, 4) + kappa;
    let epsilon_over_k = tc / 1.2593;
    let (rho_molcm3, t) = (x.rho() / 1e6, x.t());
    let t_star = t / epsilon_over_k;
    let omega22 = 1.16145 * math::powf(t_star, -0.14874)
        + 0.52487 * math::exp(-0.77320 * t_star)
        + 2.16178 * math::exp(-2.43787 * t_star)
        - 6.435e-4 * math::powf(t_star, 0.14874) * math::sin(18.0323 * math::powf(t_star, -0.76830) - 7.27371);
    let eta0_p = 4.0785e-5 * math::sqrt(m * t) / (math::powf(vc, 2.0 / 3.0) * omega22) * f_c;
    let y = rho_molcm3 * vc / 6.0;
    let g1 = (1.0 - 0.5 * y) / pow_n(1.0 - y, 3);
    let g2 = (a[0] * (1.0 - math::exp(-a[3] * y)) / y + a[1] * g1 * math::exp(a[4] * y) + a[2] * g1)
        / (a[0] * a[3] + a[1] + a[2]);
    let eta_k_p = eta0_p * (1.0 / g2 + a[5] * y);
    let eta_p_p = (36.344e-6 * math::sqrt(m * tc) / math::powf(vc, 2.0 / 3.0))
        * a[6]
        * pow_n(y, 2)
        * g2
        * math::exp(a[7] + a[8] / t_star + a[9] / pow_n(t_star, 2));
    // Poise to Pa s.
    let total = (eta_k_p + eta_p_p) / 10.0;
    let dilute = eta0_p / 10.0;
    ViscosityStages { dilute, initial_density: 0.0, residual: total - dilute, critical: 0.0 }
}

/// R23's conductivity, Shan, Penoncello and Jacobsen 2000 (`TransportRoutines.cpp:945-970`).
pub(crate) fn r23_conductivity(x: &TransportInputs<'_>) -> ConductivityStages {
    let lambda_dg = -2.5370 + 0.05366 * x.t();
    // ΔG* = 2508.58 J/mol.
    let [dilute, residual, critical] = shan_blend(x, lambda_dg, (0.14914, 301.7111050440735, 68.345), 0.94215, 25.0);
    ConductivityStages { dilute: dilute / 1e3, residual: residual / 1e3, critical: critical / 1e3 }
}

/// Helium's conductivity, Hands and Arp 1981 as REFPROP codes it (`TransportRoutines.cpp:1005-1061`) but for its
/// critical term's scaled equation, which is as printed (HE1): λ₀ + λ_e, and for 3.5 < T < 12 K the critical term, which
/// reads helium's own η, `eta`, Pa s, κ_T and (∂p/∂T)_ρ, with REFPROP's two unexplained factors.
pub(crate) fn helium_conductivity(x: &TransportInputs<'_>, eta: f64) -> Result<ConductivityStages, Error> {
    let (rhoc, t, rho) = (68.0, x.t(), x.rho_mass());
    let summer = 3.739232544 / t - 2.620316969e1 / t / t + 5.982252246e1 / t / t / t - 4.926397634e1 / t / t / t / t;
    let lambda_0 = 2.7870034e-3 * math::powf(t, 7.034007057e-1) * math::exp(summer);
    let c = [1.862970530e-4, -7.275964435e-7, -1.427549651e-4, 3.290833592e-5, -5.213335363e-8, 4.492659933e-8];
    let c = [c[0], c[1], c[2], c[3], c[4], c[5], -5.924416513e-9, 7.087321137e-6, -6.013335678e-6, 8.067145814e-7];
    let c = [c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7], c[8], c[9], 3.995125013e-7];
    let (t13, t23) = (math::powf(t, 1.0 / 3.0), math::powf(t, 2.0 / 3.0));
    let lambda_e = (c[0] + c[1] * t + c[2] * t13 + c[3] * t23) * rho
        + (c[4] + c[5] * t13 + c[6] * t23) * rho * rho * rho
        + (c[7] + c[8] * t13 + c[9] * t23 + c[10] / t) * rho * rho * math::ln(rho / rhoc);
    let critical = if 3.5 < t && t < 12.0 { helium_critical(x, eta, rho)? } else { 0.0 };
    Ok(ConductivityStages { dilute: lambda_0, residual: lambda_e, critical })
}

/// Helium's critical term (3.5 < T < 12 K), W/(m K): κ_T blended inside W ≤ 1 with the scaled equation's, which is
/// evaluated as printed (HE1, DIV-0035; REFPROP's code, which CoolProp copies, mistypes it, and its blend is negative in
/// two lobes either side of the critical isochore from 5.20 to 5.99 K, where CoolProp's λ is NaN). Its ρ_c, which that
/// code mistypes, also sets the blend's bounds and the damping. The blend, its bounds, the damping and REFPROP's two
/// unexplained factors are as CoolProp codes them: no printed source gives Hands and Arp's. A λ that is not finite is
/// refused as `Undefined` (D12): exactly on the scaled equation's critical isochore x is infinite.
fn helium_critical(x: &TransportInputs<'_>, eta: f64, rho: f64) -> Result<f64, Error> {
    let (s, t) = (&HELIUM_SCALED, x.t());
    let (delta_t, delta_rho) = s.distance(t, rho);
    let k_t = x.isothermal_compressibility()?;
    let dpdt = x.dp_dt()?;
    let w = pow_n(delta_t / 0.2, 2) + pow_n(delta_rho / 0.25, 2);
    let k_t_bar = if w > 1.0 {
        k_t
    } else {
        let k_t_prime = 1.0 / (s.inverse_compressibility(delta_t, delta_rho) * pow_n(rho / s.rho_c, 2) * s.p_c);
        w * k_t + (1.0 - w) * k_t_prime
    };
    let lambda_c = 3.4685233e-17 * 3.726229668 * math::sqrt(k_t_bar) * pow_n(t, 2) / rho / eta
        * pow_n(dpdt, 2)
        * math::exp(-18.66 * pow_n(delta_t, 2) - 4.25 * pow_n(delta_rho, 4));
    if lambda_c.is_finite() {
        Ok(lambda_c)
    } else {
        Err(Error::Undefined { prop: Prop::Conductivity, phase: x.phase() })
    }
}

/// A critical region in the NBS scaled equation: x₀, E₁, E₂, β, γ, δ, and ρ_c (kg/m³), T_c (K) and p_c (Pa).
struct Scaled {
    x0: f64,
    e1: f64,
    e2: f64,
    beta: f64,
    gamma: f64,
    delta: f64,
    rho_c: f64,
    t_c: f64,
    p_c: f64,
}

/// Helium's: Kierstead's He⁴ data in the NBS equation, Levelt Sengers, Greer and Sengers 1976 Table 17 ("Equivalent NBS
/// equation"; T_c on T₅₈), whose ρ_c is 69.58 kg/m³ where REFPROP's code has 69.158.
const HELIUM_SCALED: Scaled = Scaled {
    x0: 0.392,
    e1: 2.8461,
    e2: 0.27156,
    beta: 0.3554,
    gamma: 1.1743,
    delta: 4.304,
    rho_c: 69.58,
    t_c: 5.18992,
    p_c: 2.2746e5,
};

impl Scaled {
    /// The reduced distances from the critical point, ΔT* = |T − T_c|/T_c and Δρ* = |ρ − ρ_c|/ρ_c (Hanley, McCarty and
    /// Haynes 1974 eq. 22).
    fn distance(&self, t: f64, rho: f64) -> (f64, f64) {
        ((1.0 - t / self.t_c).abs(), (1.0 - rho / self.rho_c).abs())
    }

    /// The NBS equation's h(x) = E₁ u (1 + E₂ u^{2β})^{(γ−1)/2β}, u = (x + x₀)/x₀, and h′(x) (Levelt Sengers et al. 1976
    /// eq. 3.1; Hanley, McCarty and Haynes 1974 eq. 21). REFPROP's code has u^{2/β}.
    fn h(&self, x: f64) -> (f64, f64) {
        let Scaled { x0, e1, e2, beta, gamma, .. } = *self;
        let (u, p) = ((x + x0) / x0, (gamma - 1.0) / (2.0 * beta));
        let u_2b = math::powf(u, 2.0 * beta);
        let v = 1.0 + e2 * u_2b;
        let h = e1 * u * math::powf(v, p);
        let dh = e1 / x0 * math::powf(v, p) + (gamma - 1.0) / x0 * e1 * e2 * u_2b * math::powf(v, p - 1.0);
        (h, dh)
    }

    /// The reduced inverse compressibility [ρ*² κ_T*]⁻¹ = |Δρ*|^{δ−1} [δh(x) − (x/β)h′(x)] at x = ΔT*/|Δρ*|^{1/β}
    /// (Hanley, McCarty and Haynes 1974 eqs. 21-22; REFPROP's code has x = (ΔT*/Δρ*)^{1/β}).
    fn inverse_compressibility(&self, delta_t: f64, delta_rho: f64) -> f64 {
        let x = delta_t / math::powf(delta_rho, 1.0 / self.beta);
        let (h, dh) = self.h(x);
        math::powf(delta_rho, self.delta - 1.0) * (self.delta * h - x / self.beta * dh)
    }
}

/// CO₂'s dilute conductivity, Huber et al. 2016 Eq. (3) (`TransportRoutines.cpp:830-838`), W/(m K), in the EOS's τ.
pub(crate) fn co2_dilute_conductivity(x: &TransportInputs<'_>, r: &Reducing) -> f64 {
    let tau = r.t / x.t();
    let l = [0.0151874307, 0.0280674040, 0.0228564190, -0.00741624210];
    let lambda_0 = math::powf(tau, -0.5) / (l[0] + l[1] * tau + l[2] * pow_n(tau, 2) + l[3] * pow_n(tau, 3));
    lambda_0 / 1000.0
}

/// Ethane's dilute conductivity, Friend, Ingham and Ely 1991 (`TransportRoutines.cpp:840-848`), W/(m K): its own dilute
/// η, `eta0`, Pa s, and the EOS's α⁰_ττ times the correlation's τ², 305.33/T, where the EOS's T_r is `r.t` (map 05 R10).
pub(crate) fn ethane_dilute_conductivity(x: &TransportInputs<'_>, r: &Reducing, eta0: f64) -> f64 {
    let (tau, t_star) = (305.33 / x.t(), x.t() / 245.0);
    let f_int = 1.7104147 - 0.6936482 / t_star;
    let tau_eos = r.t / x.t();
    let alpha0_tt = x.ideal_tau_tau() / (tau_eos * tau_eos);
    0.276505e-3 * (eta0 * 1e6) * (3.75 - f_int * (tau * tau * alpha0_tt + 1.5))
}

/// R123's critical conductivity, Laesecke et al. 1996 (`TransportRoutines.cpp:779-782`), W/(m K), in the EOS's τ and δ.
pub(crate) fn r123_critical(x: &TransportInputs<'_>, r: &Reducing) -> f64 {
    let (tau, delta) = (r.t / x.t(), x.rho() / r.rho);
    0.486742e-2 * math::exp(-100.0 * pow_n(tau - 1.0, 4) + -7.08535 * pow_n(delta - 1.0, 2))
}

/// The least reduced temperature |T − T_c|/T_c Tufeu's ammonia term is evaluated at: the gap between its T_c, 405.4 K,
/// and the EOS's, 405.56 K, inside which the term's divergence has no meaning (DIV-0030).
pub(crate) const AMMONIA_T_FLOOR: f64 = (405.56 - 405.4) / 405.4;

/// CoolProp's π in Tufeu's ammonia term (`TransportRoutines.cpp:983`).
#[expect(clippy::approx_constant, reason = "CoolProp's truncated π, kept for oracle parity (map 05 R12)")]
const PI_COOLPROP: f64 = 3.141592654;

/// Ammonia's critical conductivity, Tufeu et al. 1984 as CoolProp codes it (`TransportRoutines.cpp:972-1003`), W/(m K),
/// with CoolProp's k_B and π. It diverges at its own T_c, 405.4 K, at every density (map 05 R3: CoolProp gives 6.26
/// W/(m K) at 405.3999 K and 50 kg/m³, NaN at 405.4 K), so its reduced temperature is floored at `AMMONIA_T_FLOOR`:
/// CoolProp's value outside 405.24-405.56 K, finite and continuous inside (DIV-0030, ROT-111).
pub(crate) fn ammonia_critical(x: &TransportInputs<'_>) -> f64 {
    let (t, tc, rhoc, rho) = (x.t(), 405.4, 235.0, x.rho_mass());
    let (lambda, nu, gamma, delta_exp, zeta_0, a_zeta, gamma_0) = (1.2, 0.63, 1.24, 0.50, 1.34e-10, 1.0, 0.423e-8);
    let (pi, k_b) = (PI_COOLPROP, 1.3806504e-23);
    let tt = ((t - tc) / tc).abs();
    let tt = if tt < AMMONIA_T_FLOOR { AMMONIA_T_FLOOR } else { tt };
    let a_chi = a_zeta / 0.7;
    let eta_b = (2.60 + 1.6 * tt) * 1e-5;
    let dpdt = (2.18 - 0.12 / math::exp(17.8 * tt)) * 1e5;
    let x_t = 0.61 * rhoc + 16.5 * math::ln(tt);
    let isochore = lambda * (k_b * t * t)
        / (6.0 * pi * eta_b * (zeta_0 * math::powf(tt, -nu) * (1.0 + a_zeta * math::powf(tt, delta_exp))))
        * dpdt
        * dpdt
        * gamma_0
        * math::powf(tt, -gamma)
        * (1.0 + a_chi * math::powf(tt, delta_exp));
    let isochore = isochore * math::exp(-36.0 * tt * tt);
    if rho < 0.6 * rhoc {
        isochore * (x_t * x_t) / (x_t * x_t + math::powi(0.6 * rhoc - 0.96 * rhoc, 2)) * math::powi(rho, 2)
            / math::powi(0.6 * rhoc, 2)
    } else {
        isochore * (x_t * x_t) / (x_t * x_t + math::powi(rho - 0.96 * rhoc, 2))
    }
}

#[cfg(all(test, feature = "fluids-all"))]
mod tests {
    use super::*;

    /// ROT-111, map 05 R3: Ammonia's λ is finite at Tufeu's T_c, 405.4 K, where CoolProp's is NaN, and at 405.3999 K and
    /// 50 kg/m³, where CoolProp's is 6.26 W/(m K); it is continuous across the floored band, 405.24-405.56 K, and equal
    /// to the unfloored term outside it. Below the EOS's T_c, 405.56 K, the near-critical density lies in the dome,
    /// where λ is `Undefined` (M8.10): there the term is checked at the single (liquid) phase.
    #[test]
    fn ammonia_lambda_is_finite_near_405_4_k() {
        use crate::{Density, FlashOptions, Input, Phase, Prop, Temperature};
        let fluid = crate::Registry::embedded().unwrap().get("Ammonia").unwrap();
        let lambda = |t: f64, rho: f64| {
            let input = Input::dt(Density::mass(rho).unwrap(), Temperature::new(t).unwrap());
            let state = fluid.state(input).unwrap();
            let state = match state.quality() {
                Some(_) => fluid.flash(input, &FlashOptions::new().with_phase(Phase::Liquid)).unwrap(),
                None => state,
            };
            fluid.prop(&state, Prop::Conductivity).unwrap()
        };
        let dome = fluid.state(Input::dt(Density::mass(235.0).unwrap(), Temperature::new(405.4).unwrap())).unwrap();
        let undefined = Error::Undefined { prop: Prop::Conductivity, phase: Phase::TwoPhase };
        assert_eq!(fluid.prop(&dome, Prop::Conductivity), Err(undefined));
        for rho in [50.0, 100.0, 235.0] {
            let inside = [405.3999, 405.4, 405.4001].map(|t| lambda(t, rho));
            assert!(inside.iter().all(|l| l.is_finite() && *l < 1.0), "{rho}: {inside:?}");
            // Continuous at the band's edges: no jump, a microkelvin either side moves λ by about 1e-6.
            for edge in [405.4 - 405.4 * AMMONIA_T_FLOOR, 405.4 + 405.4 * AMMONIA_T_FLOOR] {
                let (a, b) = (lambda(edge - 1e-6, rho), lambda(edge + 1e-6, rho));
                assert!((a / b - 1.0).abs() < 1e-5, "{rho} at {edge}: {a} {b}");
            }
        }
        assert!(lambda(405.3999, 50.0) < 6.26 / 10.0, "about 100x below CoolProp's 6.26 W/(m K)");
    }

    /// HE1 (DIV-0035): helium's scaled equation is Kierstead's He⁴ NBS equation as Levelt Sengers, Greer and Sengers
    /// 1976 print it, Table 17 ("Equivalent NBS equation"), whose ρ_c is 69.58 kg/m³: its constants give the table's
    /// derived amplitudes B, D, Γ, Γ′ (eqs. 3.2-3.4) and Γ/Γ′, and α = 2 − β(δ + 1) and δ = 1 + γ/β, each to its printed
    /// digits.
    #[test]
    fn helium_scaled_equation_is_kiersteads() {
        let Scaled { x0, e1, e2, beta, gamma, delta, rho_c, t_c, p_c } = HELIUM_SCALED;
        assert_eq!((rho_c, t_c, p_c), (69.58, 5.18992, 2.2746e5));
        let amplitude = math::powf(x0, gamma) / e1 * math::powf(e2, (1.0 - gamma) / (2.0 * beta));
        let amplitude_prime = beta * math::powf(x0, gamma) / e1;
        let derived = [
            ("B", math::powf(x0, -beta), 1.395, 1e-3),
            ("D", e1 * math::powf(1.0 + e2, (gamma - 1.0) / (2.0 * beta)), 3.019, 1e-3),
            ("Γ", amplitude, 0.1611, 1e-4),
            ("Γ′", amplitude_prime, 0.04158, 1e-5),
            ("Γ/Γ′", amplitude / amplitude_prime, 3.87, 1e-2),
            ("α", 2.0 - beta * (delta + 1.0), 0.115, 1e-3),
            ("δ", 1.0 + gamma / beta, delta, 1e-3),
        ];
        for (name, got, printed, unit) in derived {
            assert!((got - printed).abs() <= unit / 2.0, "{name}: {got} against the printed {printed}");
        }
    }

    /// HE1: the scaled equation's reduced inverse compressibility, [ρ*² κ_T*]⁻¹ = |Δρ*|^{δ−1} [δh(x) − (x/β)h′(x)] at
    /// x = ΔT*/|Δρ*|^{1/β} (Hanley, McCarty and Haynes 1974 eqs. 21-22), obeys the scaling law (Levelt Sengers et al.
    /// 1976 eq. 2.12): it is positive throughout the region the blend reads (ΔT* ≤ 0.2, Δρ* ≤ 0.25), and homogeneous of
    /// degree β(δ − 1) under (ΔT*, Δρ*) → (s ΔT*, s^β Δρ*), since x is unchanged.
    #[test]
    fn helium_scaled_compressibility_obeys_the_scaling_law() {
        let s = &HELIUM_SCALED;
        let degree = s.beta * (s.delta - 1.0);
        let grid = (1..=40).flat_map(|i| (1..=50).map(move |j| (0.005 * f64::from(i), 0.005 * f64::from(j))));
        for (dt, dr) in grid {
            let inverse = s.inverse_compressibility(dt, dr);
            assert!(inverse > 0.0, "ΔT* {dt}, Δρ* {dr}: {inverse}");
            let scaled = s.inverse_compressibility(0.5 * dt, math::powf(0.5, s.beta) * dr);
            let ratio = scaled / (math::powf(0.5, degree) * inverse);
            assert!((ratio - 1.0).abs() < 1e-12, "ΔT* {dt}, Δρ* {dr}: {ratio}");
        }
    }

    /// HE1: h′ as Hanley, McCarty and Haynes 1974 print it (eq. 21) is the derivative of h (Levelt Sengers et al. 1976
    /// eq. 3.1): a central difference agrees to 1e-7 from x = 1e-3 to 1e6.
    #[test]
    fn helium_scaled_h_prime_is_h_s_derivative() {
        let s = &HELIUM_SCALED;
        for k in -3..=6 {
            let x = math::powi(10.0, k);
            let (step, (_, dh)) = (1e-6 * x, s.h(x));
            let central = (s.h(x + step).0 - s.h(x - step).0) / (2.0 * step);
            assert!((central / dh - 1.0).abs() < 1e-7, "x {x}: {central} against {dh}");
        }
    }

    /// Chung's dilute stage is its zero-density limit, as the module splits a whole model: at 1e-4 mol/m³, Y ≈ 5e-9, so
    /// G₂ is 1 and η_p 0 within 1e-6, and the total is the dilute stage, for both v8.0.0 Chung fluids. CoolProp files
    /// the whole η under `critical`, so the oracle checks only the total.
    #[test]
    fn chung_dilute_stage_is_the_zero_density_limit() {
        use crate::{Density, Input, Temperature};
        let registry = crate::Registry::embedded().unwrap();
        for name in ["Cyclopentane", "Isopentane"] {
            let record = registry.record(name).unwrap();
            let input = Input::dt(Density::molar(1e-4).unwrap(), Temperature::new(400.0).unwrap());
            let state = registry.get(name).unwrap().state(input).unwrap();
            let stages = crate::internal::viscosity_stages(&record, &state).unwrap();
            assert!((stages.dilute / stages.total() - 1.0).abs() < 1e-6, "{name}: {stages:?}");
        }
    }

    /// The higher-order CO₂ term's triple point is the EOS's (`HEOS.Ttriple()` in CoolProp).
    #[test]
    fn co2_triple_point_is_the_eos_s() {
        let record = crate::Registry::embedded().unwrap().record("CarbonDioxide").unwrap();
        assert_eq!(record.limits.t_triple(), Some(CO2_T_TRIPLE));
    }
}
