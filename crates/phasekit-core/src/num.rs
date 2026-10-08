//! L0 numerics: the sealed `Real` trait (math written once; D2), the transcendental choke point `math`
//! (D12), and the solver result types (D6).

use core::fmt::Debug;
use core::ops::{Add, Div, Mul, Neg, Sub};

mod jet;

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

/// The only place the library calls a transcendental function (D12, dependencies R14). Default: `std`;
/// the `libm` feature (M9) swaps in the `libm` crate. Whether that makes results bit-identical on Linux,
/// Windows and WASM is a property to prove with the M9 cross-target hash, not an assumption (dependencies
/// §4). clippy's `disallowed-methods` rejects every `f64` transcendental elsewhere (E16).
pub mod math {
    macro_rules! choke {
        ($($(#[$doc:meta])* $name:ident => $std:ident;)*) => {$(
            $(#[$doc])*
            #[allow(clippy::disallowed_methods)]
            pub fn $name(x: f64) -> f64 {
                x.$std()
            }
        )*};
    }

    choke! {
        /// e^x.
        exp => exp;
        /// e^x − 1.
        expm1 => exp_m1;
        /// Natural logarithm.
        ln => ln;
        /// ln(1 + x).
        ln_1p => ln_1p;
        /// Square root.
        sqrt => sqrt;
        /// Hyperbolic sine.
        sinh => sinh;
        /// Hyperbolic cosine.
        cosh => cosh;
        /// Arctangent.
        atan => atan;
        /// Cosine (datagen's Chebyshev-Lobatto nodes).
        cos => cos;
    }

    /// x^y.
    #[allow(clippy::disallowed_methods)]
    pub fn powf(x: f64, y: f64) -> f64 {
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
/// largest |a_ij| before elimination) is singular. The test is relative to the row, so multiplying a row by any factor
/// changes neither the pivots chosen nor the verdict, where CoolProp's `linsolve` compares the pivot with an absolute
/// 10ε and `MatInv_2` has no zero-determinant guard (map 03 §3.4; ROT-067).
#[cfg_attr(not(test), expect(dead_code, reason = "the pure VLE (M6.3) is the first user"))]
pub(crate) fn solve_small<const N: usize>(mut a: [[f64; N]; N], mut b: [f64; N]) -> Result<[f64; N], crate::Error> {
    const { assert!(N >= 1 && N <= 4, "solve_small solves 1 to 4 equations") };
    let singular = crate::Error::InvalidState { reason: "a singular linear system (a pivot within rounding of zero)" };
    let mut scale = [0.0; N];
    for (s, row) in scale.iter_mut().zip(&a) {
        *s = row.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        if !(*s > 0.0 && s.is_finite()) {
            return Err(singular);
        }
    }
    let tol = 64.0 * f64::EPSILON * N as f64;
    for k in 0..N {
        let size = |i: usize| a[i][k].abs() / scale[i];
        let p = (k..N).fold(k, |best, i| if size(i) > size(best) { i } else { best });
        if size(p).is_nan() || size(p) <= tol {
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
    let mut x = [0.0; N];
    for k in (0..N).rev() {
        x[k] = (k + 1..N).fold(b[k], |sum, j| sum - a[k][j] * x[j]) / a[k][k];
    }
    if x.iter().all(|v| v.is_finite()) { Ok(x) } else { Err(singular) }
}

#[cfg(test)]
mod tests {
    use super::*;

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
