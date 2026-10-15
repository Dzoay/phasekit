//! The residual Helmholtz energy of a cubic (map 06 §3.1), one kernel over [`Real`] for its value and, on [`Jet4`],
//! its derivatives (D2; ROT-218).

use phasekit_core::{DataSet, Derivs, Jet4, Order, Real, Virials};

use crate::{Alpha, Kind, Omegas, R_CODATA};

/// A cubic's residual part: its kind, constants and alpha function, and the critical point that scales a and b.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Residual {
    kind: Kind,
    omegas: Omegas,
    alpha: Alpha,
    t_c: f64,
    p_c: f64,
}

impl Residual {
    /// The cubic `kind` with `data`'s constants ([`Kind::omegas`]), `alpha`, and a and b scaled by `t_c` (K) and
    /// `p_c` (Pa).
    pub const fn new(kind: Kind, data: DataSet, alpha: Alpha, t_c: f64, p_c: f64) -> Self {
        Self { kind, omegas: kind.omegas(data), alpha, t_c, p_c }
    }

    /// The co-volume b = Ω_b R T_c/p_c, m³/mol.
    pub fn b(&self) -> f64 {
        self.omegas.b * R_CODATA * self.t_c / self.p_c
    }

    /// The attraction a(T) = Ω_a R²T_c²/p_c · α(T/T_c), Pa m⁶/mol², and da/dT.
    pub fn a(&self, t: f64) -> (f64, f64) {
        let a_c = self.omegas.a * R_CODATA * R_CODATA * self.t_c * self.t_c / self.p_c;
        let alpha = self.alpha.eval(Jet4::tau(t / self.t_c));
        (a_c * alpha.value(), a_c * alpha.derivative(1, 0).unwrap_or(f64::NAN) / self.t_c)
    }

    /// The `A_ij` of α^r at (T, ρ) to `order`, from `Jet4` at τ = T_c/T and δ = ρRT_c/p_c; the bundle is
    /// reducing-invariant, so these internal variables never leave the crate (D3).
    pub fn derivs(&self, t: f64, rho: f64, order: Order) -> Derivs {
        let (tau, delta) = (self.t_c / t, rho * R_CODATA * self.t_c / self.p_c);
        let (tau_jet, delta_jet) = (Jet4::tau(tau), Jet4::delta(delta));
        let alpha = self.alpha.eval(Jet4::constant(1.0) / tau_jet);
        let full = alphar(self.kind, self.omegas, alpha, tau_jet, delta_jet).derivs(tau, delta);
        Derivs::from_fn(order, |i, j| full.get(i, j).unwrap_or(f64::NAN))
    }

    /// The pole of α^r, ρ = 1/b: no state exists at or beyond it.
    pub fn rho_max(&self) -> f64 {
        1.0 / self.b()
    }

    /// The exact virials of α^r = (b − a/RT)ρ + (b² + (Δ₁ + Δ₂)ab/RT)ρ²/2 + …: B = b − a/RT and
    /// C = b² + (Δ₁ + Δ₂)ab/RT (E4).
    pub fn virials(&self, t: f64) -> Virials {
        let ((a, da), b, (d1, d2)) = (self.a(t), self.b(), self.kind.deltas());
        let rt = R_CODATA * t;
        Virials {
            b: b - a / rt,
            c: b * b + (d1 + d2) * a * b / rt,
            db_dt: (a / t - da) / rt,
            dc_dt: (d1 + d2) * b * (da - a / t) / rt,
        }
    }
}

/// α^r at τ = T_c/T and δ = ρRT_c/p_c: −ln(1 − η) − (Ω_a/Ω_b) α τ φ(η), with η = Ω_b δ = bρ and
/// φ(η) = [ln(1 + Δ₁η) − ln(1 + Δ₂η)]/(Δ₁ − Δ₂), whose limit at Δ₁ = Δ₂ = 0 is η (van der Waals).
pub(crate) fn alphar<R: Real>(kind: Kind, omegas: Omegas, alpha: R, tau: R, delta: R) -> R {
    let eta = delta * omegas.b;
    let phi = match kind.deltas() {
        (d1, d2) if d1 == d2 => eta,
        (d1, d2) => ((eta * d1).ln_1p() - (eta * d2).ln_1p()) * (1.0 / (d1 - d2)),
    };
    -(-eta).ln_1p() - alpha * tau * phi * (omegas.a / omegas.b)
}
