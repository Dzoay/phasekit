//! The multi-fluid model's reducing functions (Kunz and Wagner 2012, eqs. 7.9-7.10; GERG TM15 §7.2; map 04 §3.2):
//!
//! Y_r(x) = Σ_i x_i² Y_i + Σ_{i<j} 2 x_i x_j β_Y,ij γ_Y,ij Y_ij (x_i + x_j)/(β_Y,ij² x_i + x_j),
//!
//! with Y = T (Y_ij = √(T_i T_j)) and Y = v (Y_ij = (v_i^⅓ + v_j^⅓)³/8), T_i and v_i = 1/ρ_i each component's own
//! reducing state, as CoolProp 8.0.0 takes them (`ReducingFunctions.cpp:244-260, 565-568`), the cube roots as its
//! `pow(v, 1/3)`. A pair is oriented to its components' sorted CAS numbers, so i is the first. Lemmon's ξ (K) and ζ
//! (m³/mol) pairs become β = 1, γ_T = (T_i + T_j + ξ)/(2√(T_i T_j)) and γ_v = (v_i + v_j + ζ)/((v_i^⅓ + v_j^⅓)³/4),
//! which turns the sum into Lemmon's Σ x_i x_j (T_i + T_j + ξ) (`ReducingFunctions.h:695-706`).

use phasekit_core::math;

use crate::data::Reducing as Parameters;
use crate::taylor::Taylor;

/// One pair's terms, i < j in component order (the pair's CAS order may be the other way round: then β is inverted).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pair {
    i: usize,
    j: usize,
    /// β_T, 2 β_T γ_T √(T_i T_j).
    t: [f64; 2],
    /// β_v, 2 β_v γ_v (v_i^⅓ + v_j^⅓)³/8.
    v: [f64; 2],
}

/// The reducing functions of a set of components.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReducingFunctions {
    t: Box<[f64]>,
    v: Box<[f64]>,
    pairs: Box<[Pair]>,
}

/// A component's reducing temperature (K) and density (mol/m³).
pub(crate) type State = (f64, f64);

impl ReducingFunctions {
    /// From the components' reducing states and, for each pair (i < j), its parameters oriented to (first, second)
    /// as stored and whether that order is (j, i).
    pub(crate) fn new(states: &[State], pairs: &[(usize, usize, Parameters, bool)]) -> ReducingFunctions {
        let t: Box<[f64]> = states.iter().map(|s| s.0).collect();
        let v: Box<[f64]> = states.iter().map(|s| 1.0 / s.1).collect();
        let pairs = pairs
            .iter()
            .map(|&(i, j, parameters, swapped)| {
                let (ti, tj, vi, vj) = (t[i], t[j], v[i], v[j]);
                let t_ij = math::sqrt(ti * tj);
                let cube = math::powi(math::powf(vi, 1.0 / 3.0) + math::powf(vj, 1.0 / 3.0), 3);
                let [bt, gt, bv, gv] = match parameters {
                    Parameters::Gerg(p) => p,
                    Parameters::Lemmon([xi, zeta]) => {
                        [1.0, (ti + tj + xi) / (2.0 * t_ij), 1.0, (vi + vj + zeta) / (cube / 4.0)]
                    }
                };
                let (bt, bv) = if swapped { (1.0 / bt, 1.0 / bv) } else { (bt, bv) };
                Pair { i, j, t: [bt, 2.0 * bt * gt * t_ij], v: [bv, 2.0 * bv * gv * cube / 8.0] }
            })
            .collect();
        ReducingFunctions { t, v, pairs }
    }

    /// T_r(x), K.
    pub(crate) fn t_r(&self, x: &[f64]) -> f64 {
        self.reduce(x, &self.t, |p| p.t)
    }

    /// ρ_r(x) = 1/v_r(x), mol/m³.
    pub(crate) fn rho_r(&self, x: &[f64]) -> f64 {
        1.0 / self.reduce(x, &self.v, |p| p.v)
    }

    /// T_r and v_r = 1/ρ_r on Taylor numbers of the mole fractions (PLAN.md M13.6), the same sums; a pair of two absent
    /// components contributes nothing.
    pub(crate) fn reduce_taylor(&self, x: &[Taylor]) -> [Taylor; 2] {
        [(&self.t, (|p: &Pair| p.t) as fn(&Pair) -> [f64; 2]), (&self.v, |p: &Pair| p.v)].map(|(y, of)| {
            let mut sum = Taylor::constant(x[0].vars(), x[0].order(), 0.0);
            for (xi, yi) in x.iter().zip(y.iter()) {
                sum.add_scaled(*yi, &(xi * xi));
            }
            for p in self.pairs.iter() {
                let ([beta, c], xi, xj) = (of(p), &x[p.i], &x[p.j]);
                let denominator = &xi.scale(beta * beta) + xj;
                if denominator.value() != 0.0 {
                    sum.add_scaled(c, &(&(xi * xj) * &(xi + xj)).divide(&denominator));
                }
            }
            sum
        })
    }

    fn reduce(&self, x: &[f64], y: &[f64], of: impl Fn(&Pair) -> [f64; 2]) -> f64 {
        let pure: f64 = x.iter().zip(y).map(|(x, y)| x * x * y).sum();
        let mixed: f64 = self
            .pairs
            .iter()
            .map(|p| {
                let ([beta, c], xi, xj) = (of(p), x[p.i], x[p.j]);
                // Two absent components (x_i = x_j = 0) contribute nothing, not 0/0.
                let denominator = beta * beta * xi + xj;
                if denominator == 0.0 { 0.0 } else { c * xi * xj * (xi + xj) / denominator }
            })
            .sum();
        pure + mixed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At a ternary composition, where x_i + x_j ≠ 1 (a binary's x_i + x_j = 1 hides the pair factor's sum), T_r and
    /// v_r are GERG's sum written again term by term, with one pair given in the other order (β inverted), and a
    /// Lemmon pair's terms are Lemmon's own Σ x_i x_j ξ_ij added to the linear Σ x_i T_i (and ζ to Σ x_i v_i).
    #[test]
    fn a_ternary_reducing_state_is_its_formula() {
        let states = [(300.0, 5_000.0), (350.0, 4_000.0), (400.0, 3_000.0)];
        let (gerg_01, gerg_02) = ([1.02, 1.05, 0.98, 1.01], [0.97, 0.99, 1.03, 1.04]);
        let (xi, zeta) = (12.0, 1e-5);
        let pairs = [
            (0, 1, Parameters::Gerg(gerg_01), false),
            (0, 2, Parameters::Gerg(gerg_02), true),
            (1, 2, Parameters::Lemmon([xi, zeta]), false),
        ];
        let f = ReducingFunctions::new(&states, &pairs);
        let x = [0.2, 0.3, 0.5];
        let (t, v) = (states.map(|s| s.0), states.map(|s| 1.0 / s.1));
        let gerg = |i: usize, j: usize, [beta, gamma]: [f64; 2], mean: f64| {
            2.0 * x[i] * x[j] * beta * gamma * mean * (x[i] + x[j]) / (beta * beta * x[i] + x[j])
        };
        let cube = |i: usize, j: usize| math::powi(math::powf(v[i], 1.0 / 3.0) + math::powf(v[j], 1.0 / 3.0), 3) / 8.0;
        let pure = |y: [f64; 3]| (0..3).map(|i| x[i] * x[i] * y[i]).sum::<f64>();
        // Lemmon's pair term, x₁x₂(y₁ + y₂ + ξ): with the pure sum and Σ x = 1, his Σ x_i y_i + Σ x_i x_j ξ_ij.
        let lemmon = |y: [f64; 3], extra: f64| x[1] * x[2] * (y[1] + y[2] + extra);
        let t_r = pure(t)
            + gerg(0, 1, [gerg_01[0], gerg_01[1]], math::sqrt(t[0] * t[1]))
            + gerg(0, 2, [1.0 / gerg_02[0], gerg_02[1]], math::sqrt(t[0] * t[2]))
            + lemmon(t, xi);
        let v_r = pure(v)
            + gerg(0, 1, [gerg_01[2], gerg_01[3]], cube(0, 1))
            + gerg(0, 2, [1.0 / gerg_02[2], gerg_02[3]], cube(0, 2))
            + lemmon(v, zeta);
        assert!((f.t_r(&x) / t_r - 1.0).abs() < 1e-14, "{} against {t_r}", f.t_r(&x));
        assert!((f.rho_r(&x) * v_r - 1.0).abs() < 1e-14, "{} against {}", f.rho_r(&x), 1.0 / v_r);
    }
}
