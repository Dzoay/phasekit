//! The form regions 2 and 5 share (IAPWS R7-97(2012) Eqs. (15) and (32)): γ(π, τ) = γ⁰ + γ^r with
//! γ⁰ = ln π + Σ n⁰ᵢ τ^J⁰ᵢ and γ^r = Σ nᵢ π^Iᵢ (τ − τ₀)^Jᵢ, π = p/p* and τ = T*/T, and their derivatives to order 2
//! (Tables 13 and 14, 40 and 41) from one pass over each part's terms (ROT-243). Region 2 has τ₀ = 0.5, region 5 none.

use phasekit_core::{GibbsDerivs, math};

use super::{MOLAR_MASS, R};

/// One region's constants: p* (Pa), T* (K), τ₀, and its terms, (J⁰ᵢ, n⁰ᵢ) and (Iᵢ, Jᵢ, nᵢ).
pub(super) struct Steam {
    pub p_star: f64,
    pub t_star: f64,
    pub tau_0: f64,
    pub ideal: &'static [(i32, f64)],
    pub residual: &'static [(i32, i32, f64)],
}

impl Steam {
    /// γ and γ_π, γ_ππ, γ_τ, γ_ττ, γ_πτ at (π, τ): γ⁰'s, whose π-part is ln π alone, plus γ^r's, each residual term
    /// n π^I y^J (y = τ − τ₀ > 0 in the region) entering them divided by π and y as its exponents ask.
    fn gamma(&self, pi: f64, tau: f64) -> [f64; 6] {
        let mut out = [math::ln(pi), 1.0 / pi, -1.0 / (pi * pi), 0.0, 0.0, 0.0];
        for &(j, n) in self.ideal {
            let (term, fj) = (n * math::powi(tau, j), f64::from(j));
            out[0] += term;
            out[3] += fj * term / tau;
            out[4] += fj * (fj - 1.0) * term / (tau * tau);
        }
        let y = tau - self.tau_0;
        for &(i, j, n) in self.residual {
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
    pub(super) fn evaluate(&self, t: f64, p: f64) -> GibbsDerivs {
        let (pi, tau) = (p / self.p_star, self.t_star / t);
        let [g, g_pi, g_pipi, g_tau, g_tautau, g_pitau] = self.gamma(pi, tau);
        let r = R * MOLAR_MASS;
        GibbsDerivs {
            g: r * t * g,
            g_t: r * (g - tau * g_tau),
            g_p: r * t * g_pi / self.p_star,
            g_tt: r * tau * tau * g_tautau / t,
            g_tp: r * (g_pi - tau * g_pitau) / self.p_star,
            g_pp: r * t * g_pipi / (self.p_star * self.p_star),
        }
    }
}
