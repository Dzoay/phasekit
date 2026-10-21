//! Ice Ih's Gibbs energy, IAPWS R10-06(2009) Eq. (1) with Table 2's coefficients, and its five derivatives (Table 4),
//! specific (per kilogram): g(T, p) = g₀(p) − s₀T_tτ + T_t Re Σₖ rₖ F(tₖ, τ), F(t, τ) = (t − τ) ln(t − τ) +
//! (t + τ) ln(t + τ) − 2t ln t − τ²/t, τ = T/T_t, with r₁ constant and r₂ and g₀ polynomials in π − π₀, π = p/p_t.

use std::f64::consts::{FRAC_PI_2, PI};
use std::ops::{Add, Mul, Sub};

use phasekit_core::math;

/// T_t, K, p_t and p₀, Pa (Table 1).
pub(super) const T_T: f64 = 273.16;
const P_T: f64 = 611.657;
const P_0: f64 = 101_325.0;

/// g₀₀ of the 2009 revision (Table 2) and of the 2006 release, which v8.0.0 keeps (map 07 C1; DIV-0066), J/kg.
pub(super) const G00_2009: f64 = -0.632_020_233_335_886e6;
pub(super) const G00_2006: f64 = -0.632_020_233_449_497e6;

/// g₀₁ … g₀₄, J/kg (Table 2).
const G0: [f64; 4] =
    [0.655_022_213_658_955, -0.189_369_929_326_131e-7, 0.339_746_123_271_053e-14, -0.556_464_869_058_991e-21];

/// s₀, the "IAPWS-95" value (Table 2), J/(kg K): ice on IAPWS-95's zero of u and s (Eq. (2)).
const S0: f64 = -0.332_733_756_492_168e4;

/// t₁, r₁ (J/(kg K)), t₂ and r₂₀ … r₂₂ (J/(kg K)) (Table 2).
const T1: C = C::new(0.368_017_112_855_051e-1, 0.510_878_114_959_572e-1);
const R1: C = C::new(0.447_050_716_285_388e2, 0.656_876_847_463_481e2);
const T2: C = C::new(0.337_315_741_065_416, 0.335_449_415_919_309);
const R2: [C; 3] = [
    C::new(-0.725_974_574_329_220e2, -0.781_008_427_112_870e2),
    C::new(-0.557_107_698_030_123e-4, 0.464_578_634_580_806e-4),
    C::new(0.234_801_409_215_913e-10, -0.285_651_142_904_972e-10),
];

/// A complex number, enough of one for Eq. (1): the release's ln is the principal value (§3).
#[derive(Clone, Copy, Debug, PartialEq)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const fn new(re: f64, im: f64) -> Self {
        C { re, im }
    }

    fn real(re: f64) -> Self {
        C::new(re, 0.0)
    }

    /// The principal value: ln |z| + i arg z, arg z in (−π, π].
    fn ln(self) -> Self {
        C::new(0.5 * math::ln(self.re * self.re + self.im * self.im), arg(self.re, self.im))
    }

    fn recip(self) -> Self {
        let d = self.re * self.re + self.im * self.im;
        C::new(self.re / d, -self.im / d)
    }
}

impl Add for C {
    type Output = C;
    fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for C {
    type Output = C;
    fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for C {
    type Output = C;
    fn mul(self, o: C) -> C {
        C::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}

/// The argument of re + i·im in (−π, π], from `math::atan` (core's `math` has no atan2).
pub(super) fn arg(re: f64, im: f64) -> f64 {
    if re > 0.0 {
        math::atan(im / re)
    } else if re < 0.0 {
        math::atan(im / re) + if im >= 0.0 { PI } else { -PI }
    } else if im > 0.0 {
        FRAC_PI_2
    } else if im < 0.0 {
        -FRAC_PI_2
    } else {
        0.0
    }
}

/// F(t, τ), ∂F/∂τ and ∂²F/∂τ².
#[allow(clippy::arithmetic_side_effects, reason = "complex f64 arithmetic: IEEE, no panic")]
fn f(t: C, tau: f64) -> [C; 3] {
    let (minus, plus, x) = (t - C::real(tau), t + C::real(tau), C::real(tau));
    let two = C::real(2.0);
    let f = minus * minus.ln() + plus * plus.ln() - two * t * t.ln() - x * x * t.recip();
    let f_t = plus.ln() - minus.ln() - two * x * t.recip();
    let f_tt = minus.recip() + plus.recip() - two * t.recip();
    [f, f_t, f_tt]
}

/// The specific Gibbs energy at `t` (K) and `p` (Pa) with `g00`, and g_T, g_p, g_TT, g_Tp and g_pp (Table 4), in
/// `[g, g_t, g_p, g_tt, g_tp, g_pp]`, J/kg and its derivatives in K and Pa; g₀ and r₂ by Horner in d = π − π₀.
#[allow(clippy::arithmetic_side_effects, reason = "complex f64 arithmetic: IEEE, no panic")]
pub(super) fn evaluate(t: f64, p: f64, g00: f64) -> [f64; 6] {
    let (tau, d) = (t / T_T, (p - P_0) / P_T);
    let [g1, g2, g3, g4] = G0;
    let g0 = g00 + d * (g1 + d * (g2 + d * (g3 + d * g4)));
    let g0_d = g1 + d * (2.0 * g2 + d * (3.0 * g3 + d * 4.0 * g4));
    let g0_dd = 2.0 * g2 + d * (6.0 * g3 + d * 12.0 * g4);
    let [r20, r21, r22] = R2;
    let r2 = r20 + C::real(d) * (r21 + C::real(d) * r22);
    let (r2_d, r2_dd) = (r21 + C::real(2.0 * d) * r22, C::real(2.0) * r22);
    let ([f1, f1_t, f1_tt], [f2, f2_t, f2_tt]) = (f(T1, tau), f(T2, tau));
    [
        g0 - S0 * t + T_T * (R1 * f1 + r2 * f2).re,
        -S0 + (R1 * f1_t + r2 * f2_t).re,
        (g0_d + T_T * (r2_d * f2).re) / P_T,
        (R1 * f1_tt + r2 * f2_tt).re / T_T,
        (r2_d * f2_t).re / P_T,
        (g0_dd + T_T * (r2_dd * f2).re) / (P_T * P_T),
    ]
}
