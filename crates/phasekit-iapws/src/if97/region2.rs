//! Region 2, the vapour: γ(π, τ) = γ⁰ + γ^r with γ⁰ = ln π + Σ n⁰ᵢ τ^J⁰ᵢ (IAPWS R7-97(2012) Eq. (16), Table 10) and
//! γ^r = Σ nᵢ π^Iᵢ (τ − 0.5)^Jᵢ (Eq. (17), Table 11), π = p/p* and τ = T*/T, and their derivatives to order 2
//! (Tables 13 and 14) from one pass over each part's terms (ROT-243).

use phasekit_core::{GibbsDerivs, math};

use super::{MOLAR_MASS, R};

/// p*, Pa, and T*, K (Eqs. (16) and (17)).
const P_STAR: f64 = 1e6;
const T_STAR: f64 = 540.0;

/// Table 10: (J⁰ᵢ, n⁰ᵢ), with n⁰₁ and n⁰₂ the values for Eq. (16) itself, not those of the metastable-vapour
/// Eq. (18).
const IDEAL: [(i32, f64); 9] = [
    (0, -0.969_276_865_002_17e1),
    (1, 0.100_866_559_680_18e2),
    (-5, -0.560_879_112_830_20e-2),
    (-4, 0.714_527_380_814_55e-1),
    (-3, -0.407_104_982_239_28),
    (-2, 0.142_408_191_714_44e1),
    (-1, -0.438_395_113_194_50e1),
    (2, -0.284_086_324_607_72),
    (3, 0.212_684_637_533_07e-1),
];

/// Table 11: (Iᵢ, Jᵢ, nᵢ).
const RESIDUAL: [(i32, i32, f64); 43] = [
    (1, 0, -0.177_317_424_732_13e-2),
    (1, 1, -0.178_348_622_923_58e-1),
    (1, 2, -0.459_960_136_963_65e-1),
    (1, 3, -0.575_812_590_834_32e-1),
    (1, 6, -0.503_252_787_279_30e-1),
    (2, 1, -0.330_326_416_702_03e-4),
    (2, 2, -0.189_489_875_163_15e-3),
    (2, 4, -0.393_927_772_433_55e-2),
    (2, 7, -0.437_972_956_505_73e-1),
    (2, 36, -0.266_745_479_140_87e-4),
    (3, 0, 0.204_817_376_923_09e-7),
    (3, 1, 0.438_706_672_844_35e-6),
    (3, 3, -0.322_776_772_385_70e-4),
    (3, 6, -0.150_339_245_421_48e-2),
    (3, 35, -0.406_682_535_626_49e-1),
    (4, 1, -0.788_473_095_593_67e-9),
    (4, 2, 0.127_907_178_522_85e-7),
    (4, 3, 0.482_253_727_185_07e-6),
    (5, 7, 0.229_220_763_376_61e-5),
    (6, 3, -0.167_147_664_510_61e-10),
    (6, 16, -0.211_714_723_213_55e-2),
    (6, 35, -0.238_957_419_341_04e2),
    (7, 0, -0.590_595_643_242_70e-17),
    (7, 11, -0.126_218_088_991_01e-5),
    (7, 25, -0.389_468_424_357_39e-1),
    (8, 8, 0.112_562_113_604_59e-10),
    (8, 36, -0.823_113_408_979_98e1),
    (9, 13, 0.198_097_128_020_88e-7),
    (10, 4, 0.104_069_652_101_74e-18),
    (10, 10, -0.102_347_470_959_29e-12),
    (10, 14, -0.100_181_793_795_11e-8),
    (16, 29, -0.808_829_086_469_85e-10),
    (16, 50, 0.106_930_318_794_09),
    (18, 57, -0.336_622_505_741_71),
    (20, 20, 0.891_858_453_554_21e-24),
    (20, 35, 0.306_293_168_762_32e-12),
    (20, 48, -0.420_024_676_982_08e-5),
    (21, 21, -0.590_560_296_856_39e-25),
    (22, 53, 0.378_269_476_134_57e-5),
    (23, 39, -0.127_686_089_346_81e-14),
    (24, 26, 0.730_876_105_950_61e-28),
    (24, 40, 0.554_147_153_507_78e-16),
    (24, 58, -0.943_697_072_412_10e-6),
];

/// γ and γ_π, γ_ππ, γ_τ, γ_ττ, γ_πτ at (π, τ): γ⁰'s (Table 13), whose π-part is ln π alone, plus γ^r's (Table 14),
/// each residual term n π^I y^J (y = τ − 0.5 > 0 in the region) entering them divided by π and y as its exponents ask.
fn gamma(pi: f64, tau: f64) -> [f64; 6] {
    let mut out = [math::ln(pi), 1.0 / pi, -1.0 / (pi * pi), 0.0, 0.0, 0.0];
    for &(j, n) in &IDEAL {
        let (term, fj) = (n * math::powi(tau, j), f64::from(j));
        out[0] += term;
        out[3] += fj * term / tau;
        out[4] += fj * (fj - 1.0) * term / (tau * tau);
    }
    let y = tau - 0.5;
    for &(i, j, n) in &RESIDUAL {
        let term = n * math::powi(pi, i) * math::powi(y, j);
        let (fi, fj) = (f64::from(i), f64::from(j));
        let parts = [
            term,
            fi * term / pi,
            fi * (fi - 1.0) * term / (pi * pi),
            fj * term / y,
            fj * (fj - 1.0) * term / (y * y),
            fi * fj * term / (pi * y),
        ];
        for (sum, part) in out.iter_mut().zip(parts) {
            *sum += part;
        }
    }
    out
}

/// The molar Gibbs energy at `t` (K) and `p` (Pa) and its five derivatives, as region 1's (`region1::evaluate`).
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
