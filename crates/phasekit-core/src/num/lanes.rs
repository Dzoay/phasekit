//! `Lanes<W>`: W states side by side (PLAN.md M12.2; ARCHITECTURE.md §7). Each operation is the `f64` operation on
//! every lane, and each transcendental goes through `math` lane by lane, so a model written over `Real` gives on lane
//! `k` the bits it gives on `f64` at state `k`. The operations are `#[inline(always)]` loops over `[f64; W]`, which
//! LLVM vectorises where they are inlined (fearless_simd's vector types need a token `Real::from_f64` cannot carry).

use core::ops::{Add, Div, Mul, Neg, Sub};

use super::{Real, math, sealed};

#[cfg(feature = "libm")]
mod exp;

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
    /// The `libm` crate's `exp` ported to lanes with the `libm` backend (PLAN.md M12.5); std's, per lane, without it.
    #[inline(always)]
    fn exp(self) -> Self {
        #[cfg(feature = "libm")]
        return exp::exp(self);
        #[cfg(not(feature = "libm"))]
        self.map(math::exp)
    }
    #[inline(always)]
    fn expm1(self) -> Self {
        self.map(math::expm1)
    }
    #[inline(always)]
    fn ln(self) -> Self {
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
    #[inline(always)]
    fn powf(self, y: f64) -> Self {
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
