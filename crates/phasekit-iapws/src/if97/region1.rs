//! Region 1, the liquid: the dimensionless Gibbs energy γ(π, τ) = Σ nᵢ (7.1 − π)^Iᵢ (τ − 1.222)^Jᵢ, π = p/p* and
//! τ = T*/T (IAPWS R7-97(2012) Eq. (7), Table 2), and its derivatives to order 2 (Table 4) from one pass over the 34
//! terms (ROT-243).

use phasekit_core::GibbsDerivs;

use super::{MOLAR_MASS, R};

/// p*, Pa, and T*, K (Eq. (7)).
const P_STAR: f64 = 16.53e6;
const T_STAR: f64 = 1386.0;

/// Table 2: (Iᵢ, Jᵢ, nᵢ).
const TERMS: [(i32, i32, f64); 34] = [
    (0, -2, 0.146_329_712_131_67),
    (0, -1, -0.845_481_871_691_14),
    (0, 0, -0.375_636_036_720_40e1),
    (0, 1, 0.338_551_691_683_85e1),
    (0, 2, -0.957_919_633_878_72),
    (0, 3, 0.157_720_385_132_28),
    (0, 4, -0.166_164_171_995_01e-1),
    (0, 5, 0.812_146_299_835_68e-3),
    (1, -9, 0.283_190_801_238_04e-3),
    (1, -7, -0.607_063_015_658_74e-3),
    (1, -1, -0.189_900_682_184_19e-1),
    (1, 0, -0.325_297_487_705_05e-1),
    (1, 1, -0.218_417_171_754_14e-1),
    (1, 3, -0.528_383_579_699_30e-4),
    (2, -3, -0.471_843_210_732_67e-3),
    (2, 0, -0.300_017_807_930_26e-3),
    (2, 1, 0.476_613_939_069_87e-4),
    (2, 3, -0.441_418_453_308_46e-5),
    (2, 17, -0.726_949_962_975_94e-15),
    (3, -4, -0.316_796_448_450_54e-4),
    (3, 0, -0.282_707_979_853_12e-5),
    (3, 6, -0.852_051_281_201_03e-9),
    (4, -5, -0.224_252_819_080_00e-5),
    (4, -2, -0.651_712_228_956_01e-6),
    (4, 10, -0.143_417_299_379_24e-12),
    (5, -8, -0.405_169_968_601_17e-6),
    (8, -11, -0.127_343_017_416_41e-8),
    (8, -6, -0.174_248_712_306_34e-9),
    (21, -29, -0.687_621_312_955_31e-18),
    (23, -31, 0.144_783_078_285_21e-19),
    (29, -38, 0.263_357_816_627_95e-22),
    (30, -39, -0.119_476_226_400_71e-22),
    (31, -40, 0.182_280_945_814_04e-23),
    (32, -41, -0.935_370_872_924_58e-25),
];

/// γ and its derivatives γ_π, γ_ππ, γ_τ, γ_ττ and γ_πτ at (π, τ) (Table 4), each term n x^I y^J (x = 7.1 − π,
/// y = τ − 1.222, both at least 1 inside the region) entering them divided by x and y as its exponents ask.
fn gamma(pi: f64, tau: f64) -> [f64; 6] {
    let (x, y) = (7.1 - pi, tau - 1.222);
    let mut out = [0.0; 6];
    for &(i, j, n) in &TERMS {
        let term = n * x.powi(i) * y.powi(j);
        let (fi, fj) = (f64::from(i), f64::from(j));
        let parts = [
            term,
            -fi * term / x,
            fi * (fi - 1.0) * term / (x * x),
            fj * term / y,
            fj * (fj - 1.0) * term / (y * y),
            -fi * fj * term / (x * y),
        ];
        for (sum, part) in out.iter_mut().zip(parts) {
            *sum += part;
        }
    }
    out
}

/// The molar Gibbs energy at `t` (K) and `p` (Pa) and its five derivatives: g = RTγ, so g_p = RTγ_π/p*,
/// g_pp = RTγ_ππ/p*², g_T = R(γ − τγ_τ), g_TT = Rτ²γ_ττ/T and g_Tp = R(γ_π − τγ_πτ)/p*, times the molar mass.
pub(super) fn evaluate(t: f64, p: f64) -> GibbsDerivs {
    let (pi, tau) = (p / P_STAR, T_STAR / t);
    let [g, g_pi, g_pipi, g_tau, g_tautau, g_pitau] = gamma(pi, tau);
    let r = R * MOLAR_MASS;
    GibbsDerivs {
        g: r * t * g,
        g_t: r * (g - tau * g_tau),
        g_p: r * t * g_pi / P_STAR,
        g_tt: r * tau * tau * g_tautau / t,
        g_tp: r * (g_pi - tau * g_pitau) / P_STAR,
        g_pp: r * t * g_pipi / (P_STAR * P_STAR),
    }
}
