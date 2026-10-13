//! Peng and Robinson's cubic equation of state from a fluid's critical point and acentric factor, for seeds only: the
//! density a PT solve starts from on a branch that holds one root (a supercritical state, a gas), and the (T, ρ) a P+X
//! Newton starts from where no saturated state bounds its span (issue #143; CoolProp seeds its PT flash with SRK's
//! density, `solver_rho_Tp_SRK`). Nothing it gives is an output: the Helmholtz model's Newton starts there, and the
//! acceptance gate judges the state that Newton reaches.

use super::Caloric;
use crate::num::math;

/// Peng and Robinson's constants for one fluid.
pub(super) struct Cubic {
    t_c: f64,
    kappa: f64,
    a_c: f64,
    b: f64,
    r: f64,
}

/// √2.
const SQRT_2: f64 = core::f64::consts::SQRT_2;

/// Newton steps [`Cubic::density`] may take.
const MAX_ITER: u16 = 60;

impl Cubic {
    /// The cubic of a fluid with critical temperature `t_c`, critical pressure `p_c`, acentric factor `omega` and gas
    /// constant `r`.
    pub(super) fn new(t_c: f64, p_c: f64, omega: f64, r: f64) -> Cubic {
        let kappa = 0.374_64 + 1.542_26 * omega - 0.269_92 * omega * omega;
        Cubic { t_c, kappa, a_c: 0.457_24 * (r * t_c) * (r * t_c) / p_c, b: 0.077_80 * r * t_c / p_c, r }
    }

    /// a(T) and da/dT.
    fn a(&self, t: f64) -> (f64, f64) {
        let root = math::sqrt(t / self.t_c);
        let sqrt_alpha = 1.0 + self.kappa * (1.0 - root);
        (self.a_c * sqrt_alpha * sqrt_alpha, -self.a_c * self.kappa * sqrt_alpha / math::sqrt(t * self.t_c))
    }

    /// The density at (T, p) of the cubic's largest root in Z, mol/m³: the gas's below its critical temperature, its
    /// only one above. Newton from the roots' Cauchy bound, which lies above the cubic's inflection, falls monotonically
    /// onto the largest root.
    pub(super) fn density(&self, t: f64, p: f64) -> Option<f64> {
        let ([c2, c1, c0], big_b) = self.cubic(t, p);
        let mut z = 1.0 + c2.abs().max(c1.abs()).max(c0.abs());
        for _ in 0..MAX_ITER {
            let step = (((z + c2) * z + c1) * z + c0) / ((3.0 * z + 2.0 * c2) * z + c1);
            z -= step;
            if !z.is_finite() {
                return None;
            }
            if step.abs() <= 1e-10 * z {
                return (z > big_b).then(|| p / (z * self.r * t));
            }
        }
        None
    }

    /// The coefficients of Z³ + c2·Z² + c1·Z + c0 = 0 at (T, p), and B = bp/(RT).
    fn cubic(&self, t: f64, p: f64) -> ([f64; 3], f64) {
        let rt = self.r * t;
        let (big_a, big_b) = (self.a(t).0 * p / (rt * rt), self.b * p / rt);
        let c1 = big_a - 3.0 * big_b * big_b - 2.0 * big_b;
        ([big_b - 1.0, c1, big_b * big_b * big_b + big_b * big_b - big_a * big_b], big_b)
    }

    /// x − x_ig(T, p) at (T, p) and the cubic's density `rho` there: h's and u's in J/mol, s's in J/(mol K).
    pub(super) fn departure(&self, var: Caloric, t: f64, p: f64, rho: f64) -> Option<f64> {
        let ((a, da_dt), rt) = (self.a(t), self.r * t);
        let (z, big_b) = (p / (rho * rt), self.b * p / rt);
        let log = math::ln((z + (1.0 + SQRT_2) * big_b) / (z + (1.0 - SQRT_2) * big_b));
        let u = (t * da_dt - a) / (2.0 * SQRT_2 * self.b) * log;
        let departure = match var {
            Caloric::H => u + rt * (z - 1.0),
            Caloric::S => self.r * math::ln(z - big_b) + da_dt / (2.0 * SQRT_2 * self.b) * log,
            Caloric::U => u,
        };
        departure.is_finite().then_some(departure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CO₂'s cubic: at a supercritical (T, p) and at a gas's below T_c its Z is a root of the cubic, the largest (the
    /// cubic is positive above it), and above B; at a dilute gas's (T, p) it is the ideal gas, whose departures vanish.
    #[test]
    fn the_density_is_the_largest_root_and_dilute_is_ideal() {
        let (r, t_c, p_c) = (8.314_462_618, 304.128_2, 7.377_3e6);
        let cubic = Cubic::new(t_c, p_c, 0.223_94, r);
        for (t, p) in [(320.0, 12e6), (350.0, 30e6), (280.0, 2e6), (t_c, p_c)] {
            let ([c2, c1, c0], big_b) = cubic.cubic(t, p);
            let f = |z: f64| ((z + c2) * z + c1) * z + c0;
            let z = p / (cubic.density(t, p).unwrap_or(f64::NAN) * r * t);
            assert!(f(z).abs() < 1e-9 && z > big_b, "{t} {p}: Z {z}, f {}", f(z));
            assert!((1..=100).all(|k| f(z * (1.0 + 0.01 * f64::from(k))) > 0.0), "{t} {p}: a root above Z {z}");
        }
        let (t, p) = (900.0, 1.0);
        let rho = cubic.density(t, p).unwrap_or(f64::NAN);
        assert!((rho * r * t / p - 1.0).abs() < 1e-6, "{rho}");
        for (var, scale) in [(Caloric::H, r * t), (Caloric::U, r * t), (Caloric::S, r)] {
            let departure = cubic.departure(var, t, p, rho).unwrap_or(f64::NAN);
            assert!(departure.abs() < 1e-6 * scale, "{var:?}: {departure}");
        }
    }
}
