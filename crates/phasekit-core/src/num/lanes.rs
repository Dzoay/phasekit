//! `Lanes<W>`: W states side by side (PLAN.md M12.2; ARCHITECTURE.md §7). Each operation is the `f64` operation on
//! every lane, and each transcendental goes through `math` lane by lane, so a model written over `Real` gives on lane
//! `k` the bits it gives on `f64` at state `k`. The operations are `#[inline(always)]` loops over `[f64; W]`, which
//! LLVM vectorises where they are inlined (fearless_simd's vector types need a token `Real::from_f64` cannot carry).

use core::ops::{Add, Div, Mul, Neg, Sub};

use super::{Real, math, sealed};

/// W states side by side; crate-private, like the executors that use it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Lanes<const W: usize>(pub(crate) [f64; W]);

impl<const W: usize> Lanes<W> {
    /// `f` on every lane.
    #[inline(always)]
    fn map(mut self, f: impl Fn(f64) -> f64) -> Self {
        for x in &mut self.0 {
            *x = f(*x);
        }
        self
    }

    /// `f` on every pair of lanes.
    #[inline(always)]
    fn zip(mut self, other: Self, f: impl Fn(f64, f64) -> f64) -> Self {
        for (x, y) in self.0.iter_mut().zip(other.0) {
            *x = f(*x, y);
        }
        self
    }
}

impl<const W: usize> sealed::Sealed for Lanes<W> {}

impl<const W: usize> Add for Lanes<W> {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: Self) -> Self {
        self.zip(rhs, |a, b| a + b)
    }
}

impl<const W: usize> Sub for Lanes<W> {
    type Output = Self;
    #[inline(always)]
    fn sub(self, rhs: Self) -> Self {
        self.zip(rhs, |a, b| a - b)
    }
}

impl<const W: usize> Mul for Lanes<W> {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: Self) -> Self {
        self.zip(rhs, |a, b| a * b)
    }
}

impl<const W: usize> Div for Lanes<W> {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: Self) -> Self {
        self.zip(rhs, |a, b| a / b)
    }
}

impl<const W: usize> Neg for Lanes<W> {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        self.map(|a| -a)
    }
}

impl<const W: usize> Add<f64> for Lanes<W> {
    type Output = Self;
    #[inline(always)]
    fn add(self, rhs: f64) -> Self {
        self.map(|a| a + rhs)
    }
}

impl<const W: usize> Mul<f64> for Lanes<W> {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: f64) -> Self {
        self.map(|a| a * rhs)
    }
}

impl<const W: usize> Real for Lanes<W> {
    #[inline(always)]
    fn from_f64(x: f64) -> Self {
        Self([x; W])
    }
    /// Core's own `exp` on every lane with the `libm` backend (PLAN.md M23.2); std's, per lane, without it.
    #[inline(always)]
    fn exp(self) -> Self {
        #[cfg(feature = "libm")]
        return super::elementary::exp_lanes(self);
        #[cfg(not(feature = "libm"))]
        self.map(math::exp)
    }
    /// Core's own `expm1` (the `libm` crate's, PLAN.md M12.8 and M23.5) on every lane with the `libm` backend; std's,
    /// per lane, without it.
    #[inline(always)]
    fn expm1(self) -> Self {
        #[cfg(feature = "libm")]
        return super::elementary::expm1_lanes(self);
        #[cfg(not(feature = "libm"))]
        self.map(math::expm1)
    }
    /// Core's own `ln` on every lane with the `libm` backend (PLAN.md M23.3); std's, per lane, without it.
    #[inline(always)]
    fn ln(self) -> Self {
        #[cfg(feature = "libm")]
        return super::elementary::ln_lanes(self);
        #[cfg(not(feature = "libm"))]
        self.map(math::ln)
    }
    #[inline(always)]
    fn ln_1p(self) -> Self {
        self.map(math::ln_1p)
    }
    #[inline(always)]
    fn powi(self, n: i32) -> Self {
        self.map(|x| math::powi(x, n))
    }
    /// Core's own `pow` on every lane with the `libm` backend (PLAN.md M23.4); std's, per lane, without it.
    #[inline(always)]
    fn powf(self, y: f64) -> Self {
        #[cfg(feature = "libm")]
        return super::elementary::pow_lanes(self, y);
        #[cfg(not(feature = "libm"))]
        self.map(|x| math::powf(x, y))
    }
    #[inline(always)]
    fn sqrt(self) -> Self {
        self.map(math::sqrt)
    }
    #[inline(always)]
    fn sinh(self) -> Self {
        self.map(math::sinh)
    }
    #[inline(always)]
    fn cosh(self) -> Self {
        self.map(math::cosh)
    }
    #[inline(always)]
    fn atan(self) -> Self {
        self.map(math::atan)
    }
    #[inline(always)]
    fn abs(self) -> Self {
        self.map(f64::abs)
    }
}

/// What the lane tests share (PLAN.md M12.8, M23.2, M23.3). `cfg(test)` stands alone, so cargo-mutants sees test code.
#[cfg(test)]
pub(crate) mod tests {
    /// A SplitMix64 stream.
    pub(crate) struct Rng(pub(crate) u64);

    impl Rng {
        pub(crate) fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
        pub(crate) fn uniform(&mut self, a: f64, b: f64) -> f64 {
            a + (b - a) * (self.next() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// `v`'s bits, NaN canonical: Rust leaves NaN bit patterns unspecified (VERIFICATION.md §9.4).
    pub(crate) fn canonical(v: f64) -> u64 {
        if v.is_nan() { f64::NAN.to_bits() } else { v.to_bits() }
    }
}
