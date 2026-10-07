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

/// Solver toolbox result types (D6). Every solver returns the residual evaluated AT the returned point and
/// reports exhaustion as a status, never as a silently accepted iterate (map 03 §6). Crate-private: the
/// first solver (the M5 density Newton) uses them.
#[expect(dead_code, reason = "the M5 density Newton is the first user")]
pub(crate) mod roots {
    /// Why a solver stopped.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Stop {
        /// The tolerance was met on both step and residual.
        Converged,
        /// `max_iter` was reached; the flash turns this into `Error::NoConvergence`.
        MaxIterations,
        /// The bracket stopped containing a sign change.
        BracketLost,
    }

    /// A typed tolerance: absolute tolerances gave a 6.1 % density error in CoolProp (map 03 §6).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(crate) enum Tol {
        /// |Δx| ≤ tol·|x|.
        Relative(f64),
        /// |Δx| ≤ tol.
        Absolute(f64),
        /// |Δ ln x| ≤ tol (densities spanning decades).
        LogAxis(f64),
    }

    /// A root with the residual at that root.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(crate) struct Root {
        /// The returned abscissa.
        pub(crate) x: f64,
        /// The residual evaluated at `x` (not at the previous iterate).
        pub(crate) f: f64,
        /// Iterations spent.
        pub(crate) iterations: u16,
        /// Why the solver stopped.
        pub(crate) stop: Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powi_is_a_fixed_chain() {
        assert_eq!(math::powi(2.0, 10), 1024.0);
        assert_eq!(math::powi(2.0, -2), 0.25);
        assert_eq!(math::powi(0.0, 0), 1.0);
    }
}
