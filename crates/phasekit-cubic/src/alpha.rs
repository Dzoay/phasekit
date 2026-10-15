//! The alpha function α(T) of a cubic's attraction a(T) = a_c α(T) (map 06 §3.1).

use phasekit_core::Real;

/// α as a function of θ = T/T_c, with α(1) = 1.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Alpha {
    /// α = 1: van der Waals' own.
    One,
}

impl Alpha {
    /// α(θ), generic over [`Real`] so that one formula gives the value and, on `Jet4`, its derivatives.
    pub fn eval<R: Real>(self, theta: R) -> R {
        match self {
            Alpha::One => theta * 0.0 + 1.0,
        }
    }
}
