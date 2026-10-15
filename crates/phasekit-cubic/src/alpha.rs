//! The alpha function α(T) of a cubic's attraction a(T) = a_c α(T) (map 06 §3.1).

use phasekit_core::Real;

/// α as a function of θ = T/T_c, with α(1) = 1.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Alpha {
    /// α = 1: van der Waals' own.
    One,
    /// Soave (1972): α = [1 + m(1 − √θ)]², m from the acentric factor ([`Kind::soave_m`](crate::Kind::soave_m)).
    Soave(f64),
    /// Mathias and Copeman (1983): α = [1 + c₁D + c₂D² + c₃D³]² with D = 1 − √θ, all three terms at every T as
    /// v8.0.0 applies them (map 06 §3.1; §10 Q6 asks whether the paper keeps only c₁ above T_c).
    MathiasCopeman([f64; 3]),
    /// Twu et al. (1991): α = θ^(N(M − 1)) exp[L(1 − θ^(MN))], from [L, M, N] (map 06 §3.1).
    Twu([f64; 3]),
}

impl Alpha {
    /// α(θ), generic over [`Real`] so that one formula gives the value and, on `Jet4`, its derivatives.
    pub fn eval<R: Real>(self, theta: R) -> R {
        let one = theta * 0.0 + 1.0;
        let d = || one - theta.sqrt();
        match self {
            Alpha::One => one,
            Alpha::Soave(m) => (d() * m + 1.0).powi(2),
            Alpha::MathiasCopeman([c1, c2, c3]) => {
                let d = d();
                ((d * c3 + c2) * d * d + d * c1 + 1.0).powi(2)
            }
            Alpha::Twu([l, m, n]) => theta.powf(n * (m - 1.0)) * ((one - theta.powf(m * n)) * l).exp(),
        }
    }
}
