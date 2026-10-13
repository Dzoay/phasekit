//! L0 numerics: the sealed `Real` trait (math written once; D2), the transcendental choke point `math`
//! (D12), and the solver result types (D6).

use core::fmt::Debug;
use core::ops::{Add, Div, Mul, Neg, Sub};

mod jet;

pub(crate) use jet::Jet2;
pub use jet::Jet4;

mod sealed {
    /// Only the core implements `Real`, so methods can be added later without a breaking change (S-02).
    pub trait Sealed {}
    impl Sealed for f64 {}
}

/// The scalar the model math is written over. `f64` is the reference; [`Jet4`] (bivariate, order 4)
/// gives exact derivatives; a core-local `Lanes<W>` joins only if the post-0.1 SIMD gate fires (D9).
/// Public so families outside the core can write a formula once and evaluate it on `f64` and `Jet4`;
/// sealed so nobody outside the core implements it. The public API itself stays `f64`.
pub trait Real:
    sealed::Sealed
    + Copy
    + Debug
    + Send
    + Sync
    + 'static
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + Add<f64, Output = Self>
    + Mul<f64, Output = Self>
{
    /// Embeds a constant.
    fn from_f64(x: f64) -> Self;
    /// e^x.
    fn exp(self) -> Self;
    /// e^x − 1, accurate near 0 (map 02 §9: `ln(−expm1(−x))` for Planck-Einstein terms).
    fn expm1(self) -> Self;
    /// ln x.
    fn ln(self) -> Self;
    /// ln(1 + x), accurate near 0.
    fn ln_1p(self) -> Self;
    /// x^n by a fixed multiplication chain, never the `powi` intrinsic.
    fn powi(self, n: i32) -> Self;
    /// x^y.
    fn powf(self, y: f64) -> Self;
    /// √x.
    fn sqrt(self) -> Self;
    /// sinh x (AlyLee, GERG-2004 terms; map 02 §3.2).
    fn sinh(self) -> Self;
    /// cosh x.
    fn cosh(self) -> Self;
    /// arctan x (Olchowy-Sengers critical enhancement; map 05 §3).
    fn atan(self) -> Self;
    /// |x|.
    fn abs(self) -> Self;
}

impl Real for f64 {
    fn from_f64(x: f64) -> Self {
        x
    }
    fn exp(self) -> Self {
        math::exp(self)
    }
    fn expm1(self) -> Self {
        math::expm1(self)
    }
    fn ln(self) -> Self {
        math::ln(self)
    }
    fn ln_1p(self) -> Self {
        math::ln_1p(self)
    }
    fn powi(self, n: i32) -> Self {
        math::powi(self, n)
    }
    fn powf(self, y: f64) -> Self {
        math::powf(self, y)
    }
    fn sqrt(self) -> Self {
        math::sqrt(self)
    }
    fn sinh(self) -> Self {
        math::sinh(self)
    }
    fn cosh(self) -> Self {
        math::cosh(self)
    }
    fn atan(self) -> Self {
        math::atan(self)
    }
    fn abs(self) -> Self {
        f64::abs(self)
    }
}

/// The only place the library calls a transcendental function (D12, dependencies R14). Default: `std`, the
/// platform's maths library; the opt-in `libm` feature (M9.3) swaps in the `libm` crate, the one feature that changes
/// numerics (docs/DEPENDENCIES.md T1). Whether that makes results bit-identical on Linux, Windows and WASM is a
/// property the M9.4 cross-target hash proves, not an assumption (dependencies §4). clippy's `disallowed-methods`
/// rejects every `f64` transcendental elsewhere (E16).
pub mod math {
    /// The backend this build's transcendentals run on: `"libm"` with the `libm` feature, else `"std"`.
    pub const BACKEND: &str = if cfg!(feature = "libm") { "libm" } else { "std" };

    macro_rules! choke {
        ($($(#[$doc:meta])* $name:ident => $std:ident, $libm:ident;)*) => {$(
            $(#[$doc])*
            #[allow(clippy::disallowed_methods)]
            pub fn $name(x: f64) -> f64 {
                #[cfg(feature = "libm")]
                return libm::$libm(x);
                #[cfg(not(feature = "libm"))]
                x.$std()
            }
        )*};
    }

    choke! {
        /// e^x.
        exp => exp, exp;
        /// e^x − 1.
        expm1 => exp_m1, expm1;
        /// Natural logarithm.
        ln => ln, log;
        /// ln(1 + x).
        ln_1p => ln_1p, log1p;
        /// Square root.
        sqrt => sqrt, sqrt;
        /// Hyperbolic sine.
        sinh => sinh, sinh;
        /// Hyperbolic cosine.
        cosh => cosh, cosh;
        /// Arctangent.
        atan => atan, atan;
        /// Arccosine (IAPWS R12-08's Y(ξ)).
        acos => acos, acos;
        /// Sine (IAPWS R12-08's Y(ξ)).
        sin => sin, sin;
        /// Tangent (IAPWS R12-08's Y(ξ)).
        tan => tan, tan;
        /// Cosine (datagen's Chebyshev-Lobatto nodes).
        cos => cos, cos;
    }

    /// x^y.
    #[allow(clippy::disallowed_methods)]
    pub fn powf(x: f64, y: f64) -> f64 {
        #[cfg(feature = "libm")]
        return libm::pow(x, y);
        #[cfg(not(feature = "libm"))]
        x.powf(y)
    }

    /// x^n by binary exponentiation: a fixed multiplication chain, identical on every target.
    pub fn powi(x: f64, n: i32) -> f64 {
        let mut base = if n < 0 { 1.0 / x } else { x };
        let mut e = n.unsigned_abs();
        let mut acc = 1.0;
        while e > 0 {
            if e & 1 == 1 {
                acc *= base;
            }
            base *= base;
            e >>= 1;
        }
        acc
    }
}

/// Solves A·x = b for N ≤ 4 by Gaussian elimination with scaled partial pivoting: the only matrix solve in the core
/// (the VLE and HS Newtons, M6.3 and M7; dependencies R9). A pivot within 64·ε·N of its row's scale (the row's
/// largest |a_ij| before elimination) is singular, and so is a NaN pivot or a zero row; of equally scaled pivots the
/// first row is taken. The test is relative to the row, so multiplying a row by any factor changes neither the pivots
/// chosen nor the verdict, where CoolProp's `linsolve` compares the pivot with an absolute
/// 10ε and `MatInv_2` has no zero-determinant guard (map 03 §3.4; ROT-067).
pub(crate) fn solve_small<const N: usize>(mut a: [[f64; N]; N], mut b: [f64; N]) -> Result<[f64; N], crate::Error> {
    const { assert!(N >= 1 && N <= 4, "solve_small solves 1 to 4 equations") };
    let singular = crate::Error::InvalidState { reason: "a singular linear system (a pivot within rounding of zero)" };
    // A zero row's scale is 0 and its sizes NaN, never chosen and singular at its own turn; a NaN or infinite entry
    // ends as a NaN pivot or a non-finite solution.
    let mut scale = a.map(|row| row.iter().fold(0.0_f64, |m, v| m.max(v.abs())));
    let tol = 64.0 * f64::EPSILON * N as f64;
    for k in 0..N {
        let size = |i: usize| a[i][k].abs() / scale[i];
        let p = (k..N).fold(k, |best, i| if size(i) > size(best) { i } else { best });
        // A NaN size compares false: singular too.
        let pivot_is_sound = size(p) > tol;
        if !pivot_is_sound {
            return Err(singular);
        }
        a.swap(k, p);
        b.swap(k, p);
        scale.swap(k, p);
        let pivot = a[k];
        for i in k + 1..N {
            let m = a[i][k] / pivot[k];
            for (aij, akj) in a[i][k..].iter_mut().zip(&pivot[k..]) {
                *aij -= m * akj;
            }
            b[i] -= m * b[k];
        }
    }
    // NaN until solved: back substitution reads only the x[j] it has solved.
    let mut x = [f64::NAN; N];
    for k in (0..N).rev() {
        x[k] = (k + 1..N).fold(b[k], |sum, j| sum - a[k][j] * x[j]) / a[k][k];
    }
    if x.iter().all(|v| v.is_finite()) { Ok(x) } else { Err(singular) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every one-argument function of [`math`] with its `libm` and `std` counterparts.
    #[cfg(feature = "libm")]
    #[allow(clippy::disallowed_methods, clippy::type_complexity)]
    const UNARY: [(&str, fn(f64) -> f64, fn(f64) -> f64, fn(f64) -> f64); 12] = [
        ("exp", math::exp, libm::exp, f64::exp),
        ("expm1", math::expm1, libm::expm1, f64::exp_m1),
        ("ln", math::ln, libm::log, f64::ln),
        ("ln_1p", math::ln_1p, libm::log1p, f64::ln_1p),
        ("sqrt", math::sqrt, libm::sqrt, f64::sqrt),
        ("sinh", math::sinh, libm::sinh, f64::sinh),
        ("cosh", math::cosh, libm::cosh, f64::cosh),
        ("atan", math::atan, libm::atan, f64::atan),
        ("acos", math::acos, libm::acos, f64::acos),
        ("sin", math::sin, libm::sin, f64::sin),
        ("tan", math::tan, libm::tan, f64::tan),
        ("cos", math::cos, libm::cos, f64::cos),
    ];

    /// 10⁶ inputs for [`libm_backend_matches_the_libm_crate_bitwise`]: the edges (±0, the smallest subnormal and
    /// normal, ±709.78 around e^x's overflow, ±745 around its underflow, ±1, ±π, NaN, ±∞), then uniform bit
    /// patterns (every exponent, subnormals, NaNs and infinities included) and uniform values in ±800, alternately.
    #[cfg(feature = "libm")]
    fn inputs() -> Vec<f64> {
        let edges = [
            0.0,
            5e-324,
            f64::MIN_POSITIVE,
            709.78,
            709.79,
            745.0,
            1.0,
            core::f64::consts::PI,
            1e-300,
            1e300,
            f64::MAX,
            f64::INFINITY,
        ];
        let mut xs: Vec<f64> = edges.iter().flat_map(|&x| [x, -x]).chain([f64::NAN]).collect();
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        while xs.len() < 1_000_000 {
            // SplitMix64 (VERIFICATION.md §3.1), inlined: the core has no sampler.
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            xs.push(if xs.len().is_multiple_of(2) {
                f64::from_bits(z)
            } else {
                (z >> 11) as f64 / (1u64 << 53) as f64 * 1600.0 - 800.0
            });
        }
        xs
    }

    /// PLAN.md M9.3 (dependencies R14): with the `libm` feature every function of [`math`] is the `libm` crate's, bit
    /// for bit, on 10⁶ inputs and edges; `powf` on each input against the next as exponent, and against 0.5, 2, −1,
    /// ±∞ and NaN. `powi` stays the in-house chain.
    #[cfg(feature = "libm")]
    #[test]
    fn libm_backend_matches_the_libm_crate_bitwise() {
        let xs = inputs();
        for (name, ours, theirs, _) in UNARY {
            for &x in &xs {
                assert_eq!(ours(x).to_bits(), theirs(x).to_bits(), "{name}({x:e})");
            }
        }
        let exponents = [0.5, 2.0, -1.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN];
        for (k, &x) in xs.iter().enumerate() {
            for y in exponents.into_iter().chain(xs.get(k + 1).copied()) {
                assert_eq!(math::powf(x, y).to_bits(), libm::pow(x, y).to_bits(), "powf({x:e}, {y:e})");
            }
        }
    }

    /// PLAN.md M9.3: how far `std` (the platform's maths library) and `libm` lie apart on this target, recorded (run
    /// with `--no-capture`), not asserted across targets: per function, the inputs whose results differ and the
    /// largest distance in ulps where both are finite. Asserted: `libm` is never non-finite where `std` is finite, or
    /// the reverse, away from the overflow edges (within 1e-12 relative of the largest finite result, the two may
    /// round to opposite sides of f64::MAX).
    #[cfg(feature = "libm")]
    #[test]
    #[allow(clippy::print_stdout, reason = "the report is this test's record, read with --no-capture")]
    fn std_vs_libm_ulp_report() {
        // ulps between two finite doubles of the same sign class, through their ordered integer images.
        let ordered = |x: f64| {
            let b = x.to_bits() as i64;
            if b < 0 { i64::MIN - b } else { b }
        };
        let ulps = |a: f64, b: f64| ordered(a).abs_diff(ordered(b));
        let xs = inputs();
        let report = UNARY.iter().map(|&(name, _, libm_fn, std_fn)| {
            let (mut differ, mut worst, mut worst_at) = (0, 0, 0.0);
            for &x in &xs {
                let (a, b) = (std_fn(x), libm_fn(x));
                if a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()) {
                    continue;
                }
                differ += 1;
                let edge = |v: f64| v.is_infinite() || v.abs() > f64::MAX * (1.0 - 1e-12);
                assert!(a.is_finite() == b.is_finite() || edge(a) || edge(b), "{name}({x:e}): std {a:e}, libm {b:e}");
                if a.is_finite() && b.is_finite() && ulps(a, b) > worst {
                    (worst, worst_at) = (ulps(a, b), x);
                }
            }
            format!("{name:6} {differ:7} of {} differ, at most {worst} ulp (at {worst_at:e})", xs.len())
        });
        let lines: Vec<String> = report.collect();
        println!("std vs libm on {}:\n{}", std::env::consts::ARCH, lines.join("\n"));
        assert_eq!(lines.len(), UNARY.len());
    }

    /// ROT-067 (map 03 §3.4: `MatInv_2` has no zero-determinant guard): a singular 2×2 system, a zero row and a
    /// non-finite entry are errors, never infinities or NaN.
    #[test]
    fn singular_2x2_is_an_error() {
        let singular =
            Err(crate::Error::InvalidState { reason: "a singular linear system (a pivot within rounding of zero)" });
        assert_eq!(solve_small([[1.0, 2.0], [2.0, 4.0]], [1.0, 2.0]), singular);
        assert_eq!(solve_small([[1.0, 2.0], [0.0, 0.0]], [1.0, 0.0]), singular);
        assert_eq!(solve_small([[f64::NAN, 2.0], [1.0, 1.0]], [1.0, 0.0]), singular);
        assert_eq!(solve_small([[0.0, 1.0], [1.0, 0.0]], [2.0, 3.0]), Ok([3.0, 2.0]), "a zero first pivot is swapped");
        assert_eq!(solve_small([[4.0]], [2.0]), Ok([0.5]));
        assert_eq!(solve_small([[f64::INFINITY, 1.0], [1.0, 1.0]], [1.0, 0.0]), singular);
        assert_eq!(solve_small([[1.0, f64::INFINITY], [1.0, 1.0]], [1.0, 0.0]), singular);
    }

    /// The pivot test at its threshold, 64·ε·N of the row's scale: 2⁻⁴⁵ for N = 2 exactly. A second pivot of 2⁻⁴⁵ is
    /// singular, one of 2⁻⁴⁴ is solved. Of two equally scaled pivots the first row is taken: rows (1, 1) and (−2, 2)
    /// with b = (0.1, 0.7) give (−0.12499999999999997, 0.22499999999999998), where pivoting on the second row gives
    /// x₀ = −0.125.
    #[test]
    fn pivot_threshold_and_ties() {
        let singular =
            Err(crate::Error::InvalidState { reason: "a singular linear system (a pivot within rounding of zero)" });
        let at = math::powi(2.0, -45);
        assert_eq!(solve_small([[1.0, 0.0], [1.0, at]], [1.0, 1.0]), singular);
        assert_eq!(solve_small([[1.0, 0.0], [1.0, 2.0 * at]], [1.0, 1.0]), Ok([1.0, 0.0]));
        assert_eq!(
            solve_small([[1.0, 1.0], [-2.0, 2.0]], [0.1, 0.7]),
            Ok([-0.124_999_999_999_999_97, 0.224_999_999_999_999_98])
        );
    }

    /// ROT-067 (map 03 §3.4: an absolute pivot test < 10ε is scale-dependent): the verdict and the solution do not
    /// change when rows are multiplied by 2⁻¹⁰⁰ or 2¹⁰⁰ (exact factors). A system whose third row is the sum of the
    /// others up to 1e-16 of its scale is singular at every scale; with 1e-6 it is solved at every scale.
    #[test]
    fn near_singular_3x3_is_scale_invariant() {
        let system = |gap: f64| {
            let a = [[2.0, 1.0, -1.0], [1.0, 3.0, 2.0], [3.0, 4.0, 1.0 + gap]];
            let x = [1.0, -2.0, 0.5];
            let b: [f64; 3] = core::array::from_fn(|i| (0..3).map(|j| a[i][j] * x[j]).sum());
            (a, b)
        };
        let scaled = |(mut a, mut b): ([[f64; 3]; 3], [f64; 3]), factors: [f64; 3]| {
            for i in 0..3 {
                a[i] = a[i].map(|v| v * factors[i]);
                b[i] *= factors[i];
            }
            (a, b)
        };
        let tiny = math::powi(2.0, -100);
        let huge = math::powi(2.0, 100);
        for factors in [[1.0, 1.0, 1.0], [tiny, tiny, tiny], [huge, 1.0, tiny], [1.0, huge, huge]] {
            let (a, b) = scaled(system(1e-16), factors);
            assert!(solve_small(a, b).is_err(), "{factors:?}");
            let (a, b) = scaled(system(1e-6), factors);
            let x = solve_small(a, b).unwrap();
            let want = [1.0, -2.0, 0.5];
            assert!(x.iter().zip(want).all(|(x, w)| (x - w).abs() < 1e-9), "{factors:?}: {x:?}");
        }
        let (a, b) = system(1e-6);
        assert_eq!(
            solve_small(a, b),
            solve_small(scaled(system(1e-6), [huge, tiny, 4.0]).0, scaled(system(1e-6), [huge, tiny, 4.0]).1)
        );
    }

    /// A well-conditioned 4×4 system solves to its known solution.
    #[test]
    fn four_equations_solve() {
        let a = [[4.0, -1.0, 0.0, 1.0], [-1.0, 4.0, -1.0, 0.0], [0.0, -1.0, 4.0, -1.0], [1.0, 0.0, -1.0, 4.0]];
        let x = [1.0, 2.0, -1.0, 0.5];
        let b: [f64; 4] = core::array::from_fn(|i| (0..4).map(|j| a[i][j] * x[j]).sum());
        let got = solve_small(a, b).unwrap();
        assert!(got.iter().zip(x).all(|(g, w)| (g - w).abs() < 1e-14), "{got:?}");
    }

    #[test]
    fn powi_is_a_fixed_chain() {
        assert_eq!(math::powi(2.0, 10), 1024.0);
        assert_eq!(math::powi(2.0, -2), 0.25);
        assert_eq!(math::powi(0.0, 0), 1.0);
    }
}
