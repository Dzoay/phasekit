//! Humid air's seven virial coefficients (RP-1485 §3.3), each with its first two T-derivatives: B_aa and C_aaa of
//! the dry-air part and B_ww and C_www of the water part, their models' exact virials (`HelmholtzModel::zero_density`;
//! RP-1485 takes them "according to Feistel et al.", LibHuAirProp guide §1.3, which IAPWS G11-15 is); and the cross
//! coefficients, functions of T alone, in IAPWS G8-10's forms (Eqs. (9)-(11), Table 4, T̄ = T/100 K).

use phasekit_core::math;

/// A virial coefficient at one T and its first two T-derivatives, molar SI: B in m³/mol, C in m⁶/mol².
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Coefficient {
    /// The coefficient.
    pub value: f64,
    /// Its T-derivative, per K.
    pub d_dt: f64,
    /// Its second T-derivative, per K².
    pub d2_dt2: f64,
}

/// Humid air's seven virial coefficients at one temperature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Virials {
    /// Dry air's second.
    pub b_aa: Coefficient,
    /// Dry air's third.
    pub c_aaa: Coefficient,
    /// Water's second.
    pub b_ww: Coefficient,
    /// Water's third.
    pub c_www: Coefficient,
    /// The second cross coefficient.
    pub b_aw: Coefficient,
    /// The third cross coefficient with two air molecules.
    pub c_aaw: Coefficient,
    /// The third cross coefficient with two water molecules.
    pub c_aww: Coefficient,
}

/// b* and c*, m³/mol and m⁶/mol² (G8-10 Eqs. (9)-(11)).
const B_STAR: f64 = 1e-6;
const C_STAR: f64 = 1e-6;

/// G8-10 Table 4's cᵢ and dᵢ for B_aw (Harvey and Huang 2007), which v8.0.0 has too.
const B_AW: [(f64, f64); 3] = [(0.665_687e2, -0.237), (-0.238_834e3, -1.048), (-0.176_755e3, -3.183)];

/// G8-10 Table 4's aᵢ for C_aaw (Hyland and Wexler 1983), which v8.0.0 has rescaled to T in K.
const C_AAW: [f64; 5] = [0.482_737e-3, 0.105_678e-2, -0.656_394e-2, 0.294_442e-1, -0.319_317e-1];

/// C_aww's bᵢ as v8.0.0 has them (DIV-0067, `KeepOracle`; ROT-271): G8-10 Table 4's Hyland and Wexler (1983) with b₀
/// and b₁ differing in the sixth digit (−0.10728876e2 and 0.347802e2 there). RP-1485 takes C_aaw and C_aww from Nelson
/// and Sauer (2002), which is not in hand (PLAN.md §6).
const C_AWW: [f64; 4] = [-0.107_288_7e2, 0.347_804e2, -0.383_383e2, 0.334_06e2];

/// Σ kᵢ T̄^eᵢ and its first two T-derivatives over `terms` (kᵢ, eᵢ), times `scale`.
fn power_sum(t: f64, scale: f64, terms: impl Iterator<Item = (f64, f64)>) -> Coefficient {
    let x = t / 100.0;
    let (mut value, mut d, mut d2) = (0.0, 0.0, 0.0);
    for (k, e) in terms {
        let term = k * math::powf(x, e);
        value += term;
        d += e * term / x;
        d2 += e * (e - 1.0) * term / (x * x);
    }
    Coefficient { value: scale * value, d_dt: scale * d / 100.0, d2_dt2: scale * d2 / 1e4 }
}

/// B_aw, G8-10 Eq. (9).
pub(crate) fn b_aw(t: f64) -> Coefficient {
    power_sum(t, B_STAR, B_AW.into_iter())
}

/// C_aaw, G8-10 Eq. (10): exponents 0, −1 … −4.
pub(crate) fn c_aaw(t: f64) -> Coefficient {
    power_sum(t, C_STAR, C_AAW.into_iter().zip([0.0, -1.0, -2.0, -3.0, -4.0]))
}

/// C_aww, G8-10 Eq. (11): −c* e^S, S = Σ bᵢ T̄^−ⁱ, so C′ = C S′ and C″ = C (S″ + S′²).
pub(crate) fn c_aww(t: f64) -> Coefficient {
    let s = power_sum(t, 1.0, C_AWW.into_iter().zip([0.0, -1.0, -2.0, -3.0]));
    let value = -C_STAR * math::exp(s.value);
    Coefficient { value, d_dt: value * s.d_dt, d2_dt2: value * (s.d2_dt2 + s.d_dt * s.d_dt) }
}
