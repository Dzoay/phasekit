//! A multi-fluid model's composition derivatives (PLAN.md M13.6; ROT-191, ROT-210): one mole-number AD path.
//!
//! Ψ(T, V, n) = A^r/(RT) = n α^r(τ, δ, x), with n = Σ n_i, x = n/n, τ = T_r(x)/T and δ = n v_r(x)/V, is evaluated on
//! [`Taylor`] numbers in the variables (n_1 ... n_N, T, V) at one mole of composition x and V = 1/ρ. Each component's
//! and each departure function's (τ, δ) jet to order 3 is composed with τ(n) and δ(n), which share their powers, so
//! every derivative of Ψ to order 3 comes from one evaluation, with no hand-written composition derivative (CoolProp's
//! double-count its ideal term and miss a factor of 2, ROT-191, ROT-210). Fugacities, their derivatives and partial
//! molar volumes follow from Ψ's derivatives by Michelsen and Mollerup's relations.

use phasekit_core::{Error, Order};

use crate::MultiFluid;
use crate::departure;
use crate::taylor::Taylor;

/// A variable of Ψ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Var {
    /// The mole number of component i.
    N(usize),
    /// Temperature, K.
    T,
    /// Volume, m³ (of the one mole the derivatives are taken at).
    V,
}

/// Ψ = A^r/(RT) and its derivatives in (T, V, n) at temperature T, one mole of composition x and V = 1/ρ, to the order
/// asked for (at most 3 in every variable together).
#[derive(Clone, Debug)]
pub struct Composition {
    psi: Taylor,
    components: usize,
    t: f64,
    rho: f64,
    r: f64,
}

/// The models that give composition derivatives: the multi-fluid model of the library and GERG-2008's (PLAN.md M13.6).
pub trait MixtureModel {
    /// The number of components.
    fn components(&self) -> usize;

    /// Ψ's derivatives at (T, ρ) and mole fractions `x` (one per component, summing to 1) to `order` (`Order::Four` is
    /// refused): ρ > 0, as Ψ's density derivatives are taken at fixed V.
    fn composition(&self, t: f64, rho: f64, x: &[f64], order: Order) -> Result<Composition, Error>;
}

/// Order's number, 1 to 3.
fn degree(order: Order) -> Result<usize, Error> {
    match order {
        Order::One => Ok(1),
        Order::Two => Ok(2),
        Order::Three => Ok(3),
        _ => Err(Error::InvalidInput { quantity: "order of composition derivatives", value: 4.0 }),
    }
}

/// 1/(a! b!) for a + b ≤ 3.
fn inverse_factorials(a: usize, b: usize) -> f64 {
    let f = |k: usize| [1.0, 1.0, 2.0, 6.0][k.min(3)];
    1.0 / (f(a) * f(b))
}

impl MixtureModel for MultiFluid {
    fn components(&self) -> usize {
        self.len()
    }

    fn composition(&self, t: f64, rho: f64, x: &[f64], order: Order) -> Result<Composition, Error> {
        let n = self.len();
        if x.len() != n {
            return Err(Error::InvalidInput { quantity: "number of mole fractions", value: x.len() as f64 });
        }
        if !(t > 0.0 && rho > 0.0 && t.is_finite() && rho.is_finite()) {
            return Err(Error::InvalidInput { quantity: "density for composition derivatives", value: rho });
        }
        let (m, k) = (n + 2, degree(order)?);
        let moles: Vec<Taylor> = x.iter().enumerate().map(|(i, xi)| Taylor::variable(m, k, i, *xi)).collect();
        let (temperature, volume) = (Taylor::variable(m, k, n, t), Taylor::variable(m, k, n + 1, 1.0 / rho));
        let mut total = Taylor::constant(m, k, 0.0);
        for ni in &moles {
            total.add_scaled(1.0, ni);
        }
        let inverse = total.recip();
        let fractions: Vec<Taylor> = moles.iter().map(|ni| ni * &inverse).collect();
        let [t_r, v_r] = self.reducing.reduce_taylor(&fractions);
        let tau = t_r.divide(&temperature);
        let delta = (&total * &v_r).divide(&volume);
        let (tau0, delta0) = (tau.value(), delta.value());
        // Δτ^a Δδ^b for a + b ≤ 3, shared by every part.
        let (dt, dd) = (tau.increment(), delta.increment());
        let mut powers = vec![vec![Taylor::constant(m, k, 1.0)]];
        for a in 1..=k {
            powers.push(vec![&powers[a - 1][0] * &dt]);
        }
        for (a, row) in powers.iter_mut().enumerate() {
            for b in 1..=(k - a) {
                let next = &row[b - 1] * &dd;
                row.push(next);
            }
        }
        let compose = |jet: &dyn Fn(usize, usize) -> f64| {
            let mut sum = Taylor::constant(m, k, 0.0);
            for (a, row) in powers.iter().enumerate() {
                for (b, power) in row.iter().enumerate() {
                    sum.add_scaled(jet(a, b) * inverse_factorials(a, b), power);
                }
            }
            sum
        };
        let mut alpha = Taylor::constant(m, k, 0.0);
        for (c, xi) in self.components.iter().zip(&fractions) {
            // The component's scaled derivatives at the mixture's τ and δ, unscaled.
            let model =
                c.fluid.model().helmholtz().ok_or(Error::NoModel {
                    prop: phasekit_core::Prop::Constant(phasekit_core::Constant::GasConstant),
                })?;
            let d = model.residual(c.t_r / tau0, delta0 * c.rho_r, Order::Three);
            let jet = |a: usize, b: usize| {
                d.get(a, b).unwrap_or(f64::NAN)
                    / (phasekit_core::math::powi(tau0, a as i32) * phasekit_core::math::powi(delta0, b as i32))
            };
            alpha = &alpha + &(xi * &compose(&jet));
        }
        for d in self.departures.iter() {
            let jet = departure::jet(d.function, tau0, delta0);
            let part = compose(&|a, b| jet.derivative(a, b).unwrap_or(f64::NAN));
            alpha = &alpha + &(&(&fractions[d.i] * &fractions[d.j]) * &part.scale(d.f));
        }
        let psi = &total * &alpha;
        Ok(Composition { psi, components: n, t, rho, r: self.gas_constant() })
    }
}

impl Composition {
    /// The index of `var` among Ψ's variables.
    fn index(&self, var: Var) -> usize {
        match var {
            Var::N(i) => i,
            Var::T => self.components,
            Var::V => self.components + 1,
        }
    }

    /// ∂^k Ψ / ∂vars, k ≤ the order asked for (0 beyond it): Ψ itself for none.
    pub fn psi(&self, vars: &[Var]) -> f64 {
        let idx: Vec<usize> = vars.iter().map(|v| self.index(*v)).collect();
        self.psi.derivative(&idx)
    }

    /// The number of components.
    pub fn components(&self) -> usize {
        self.components
    }

    /// The volume V = 1/ρ of the one mole, m³, at which the derivatives are taken.
    pub fn volume(&self) -> f64 {
        1.0 / self.rho
    }

    /// The compressibility factor Z = pV/(nRT) = 1 − V ∂Ψ/∂V (one mole).
    pub fn z(&self) -> f64 {
        1.0 - self.psi(&[Var::V]) / self.rho
    }

    /// The pressure, Pa.
    pub fn pressure(&self) -> f64 {
        self.rho * self.r * self.t * self.z()
    }

    /// ln φ_i = ∂Ψ/∂n_i − ln Z.
    pub fn ln_phi(&self, i: usize) -> f64 {
        self.psi(&[Var::N(i)]) - phasekit_core::math::ln(self.z())
    }

    /// (∂p/∂n_i)_(T,V), Pa/mol: RT(1/V − ∂²Ψ/∂V∂n_i).
    pub fn dp_dn(&self, i: usize) -> f64 {
        self.r * self.t * (self.rho - self.psi(&[Var::V, Var::N(i)]))
    }

    /// (∂p/∂V)_(T,n), Pa/m³: −RT(n/V² + ∂²Ψ/∂V²).
    pub fn dp_dv(&self) -> f64 {
        -self.r * self.t * (self.rho * self.rho + self.psi(&[Var::V, Var::V]))
    }

    /// (∂p/∂T)_(V,n), Pa/K: R(n/V − ∂Ψ/∂V) − RT ∂²Ψ/∂T∂V.
    pub fn dp_dt(&self) -> f64 {
        self.r * (self.rho - self.psi(&[Var::V])) - self.r * self.t * self.psi(&[Var::T, Var::V])
    }

    /// The partial molar volume v̄_i = −(∂p/∂n_i)/(∂p/∂V), m³/mol.
    pub fn partial_molar_volume(&self, i: usize) -> f64 {
        -self.dp_dn(i) / self.dp_dv()
    }

    /// n (∂ln φ_i/∂n_j)_(T,p) = n ∂²Ψ/∂n_i∂n_j + 1 + n (∂p/∂n_i)(∂p/∂n_j)/(RT ∂p/∂V) (one mole).
    pub fn n_dln_phi_dn(&self, i: usize, j: usize) -> f64 {
        let rt = self.r * self.t;
        self.psi(&[Var::N(i), Var::N(j)]) + 1.0 + self.dp_dn(i) * self.dp_dn(j) / (rt * self.dp_dv())
    }

    /// (∂ln φ_i/∂T)_(p,n), 1/K: ∂²Ψ/∂T∂n_i + 1/T − v̄_i (∂p/∂T)/(RT).
    pub fn dln_phi_dt(&self, i: usize) -> f64 {
        let rt = self.r * self.t;
        self.psi(&[Var::T, Var::N(i)]) + 1.0 / self.t - self.partial_molar_volume(i) * self.dp_dt() / rt
    }

    /// (∂ln φ_i/∂p)_(T,n), 1/Pa: v̄_i/(RT) − 1/p.
    pub fn dln_phi_dp(&self, i: usize) -> f64 {
        self.partial_molar_volume(i) / (self.r * self.t) - 1.0 / self.pressure()
    }
}

#[cfg(test)]
mod tests {
    use num_dual::{DualNum, HyperDual};
    use phasekit_core::{DataSet, Registry};

    use super::*;
    use crate::data::{Kind, Term};
    use crate::gerg::{COMPONENTS, binary, departure};

    /// Three ε directions on nested hyper-duals: one evaluation gives one mixed derivative up to order 3.
    type D3 = HyperDual<HyperDual<f64>>;

    fn seeded(x: f64, seeds: [bool; 3]) -> D3 {
        let one = |s: bool| if s { 1.0 } else { 0.0 };
        let inner = HyperDual::new(x, one(seeds[2]), 0.0, 0.0);
        let constant = |v: f64| HyperDual::new(v, 0.0, 0.0, 0.0);
        HyperDual::new(inner, constant(one(seeds[0])), constant(one(seeds[1])), constant(0.0))
    }

    /// The ∂^k of a D3 evaluation seeded in its first k directions.
    fn part(f: &D3, k: usize) -> f64 {
        match k {
            0 => f.re.re,
            1 => f.eps1.re,
            2 => f.eps1eps2.re,
            _ => f.eps1eps2.eps1,
        }
    }

    /// Σ n δ^d τ^t e^(−δ^l) (l = 0: no exponential), or GERG-2008's departure kind, written again on num-dual.
    fn terms(kind: Kind, power: usize, terms: &[Term], tau: D3, delta: D3) -> D3 {
        let mut sum = D3::from(0.0);
        for (k, t) in terms.iter().enumerate() {
            let p = delta.powi(t.d as i32) * tau.powf(t.t) * t.n;
            sum += match (kind, k >= power) {
                (Kind::Gerg2008, true) => {
                    p * (-((delta - t.epsilon).powi(2) * t.eta) - (delta - t.gamma) * t.beta).exp()
                }
                _ if t.l > 0.0 => p * (-delta.powi(t.l as i32)).exp(),
                _ => p,
            };
        }
        sum
    }

    /// GERG-2008's Ψ(T, V, n) for AGA8 components `index`, from its tables alone: reducing functions, the components'
    /// residuals and the departure functions, all on num-dual.
    fn psi(index: &[usize], n: &[D3], t: D3, v: D3) -> D3 {
        let total = n.iter().fold(D3::from(0.0), |a, b| a + *b);
        let x: Vec<D3> = n.iter().map(|ni| *ni / total).collect();
        let tc: Vec<f64> = index.iter().map(|&i| COMPONENTS[i].t_c).collect();
        let vc: Vec<f64> = index.iter().map(|&i| 1.0 / (1e3 * COMPONENTS[i].rho_c)).collect();
        let (mut t_r, mut v_r) = (D3::from(0.0), D3::from(0.0));
        for a in 0..index.len() {
            t_r += x[a] * x[a] * tc[a];
            v_r += x[a] * x[a] * vc[a];
            for b in a + 1..index.len() {
                let (pair, swapped) = binary(index[a], index[b]).unwrap();
                let invert = |beta: f64| if swapped { 1.0 / beta } else { beta };
                let (bt, bv) = (invert(pair.beta_t), invert(pair.beta_v));
                let root = |v: f64| phasekit_core::math::powf(v, 1.0 / 3.0);
                let cube = phasekit_core::math::powi(root(vc[a]) + root(vc[b]), 3) / 8.0;
                let s = x[a] + x[b];
                t_r += x[a] * x[b] * s / (x[a] * bt * bt + x[b])
                    * (2.0 * bt * pair.gamma_t * phasekit_core::math::sqrt(tc[a] * tc[b]));
                v_r += x[a] * x[b] * s / (x[a] * bv * bv + x[b]) * (2.0 * bv * pair.gamma_v * cube);
            }
        }
        let (tau, delta) = (t_r / t, total * v_r / v);
        let mut alpha = D3::from(0.0);
        for (a, &i) in index.iter().enumerate() {
            alpha += x[a] * terms(Kind::Exponential, 0, COMPONENTS[i].residual, tau, delta);
            for b in a + 1..index.len() {
                let (pair, _) = binary(i, index[b]).unwrap();
                if let Some(f) = pair.model.and_then(departure).filter(|_| pair.f != 0.0) {
                    alpha += x[a] * x[b] * terms(f.kind, f.power, f.terms, tau, delta) * pair.f;
                }
            }
        }
        total * alpha
    }

    /// ROT-191, ROT-210: every derivative of Ψ to order 3 in (n, T, V) from the Taylor path against num-dual on GERG's
    /// formulas written again, for methane/nitrogen/ethane/isobutane/n-butane (binary-specific and generalized departure
    /// functions, F ≠ 1, a reciprocal β) at a gas and a dense state, within 1e-12 of the larger of the entry and its
    /// order's largest.
    #[test]
    fn composition_derivatives_match_num_dual() {
        let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
        let (index, names) = ([0, 1, 3, 5, 6], ["Methane", "Nitrogen", "Ethane", "IsoButane", "n-Butane"]);
        let x = [0.6, 0.1, 0.15, 0.05, 0.1];
        let model = MultiFluid::gerg2008(&registry, &names).unwrap();
        for (t, rho) in [(300.0, 2_000.0), (200.0, 15_000.0)] {
            let c = model.composition(t, rho, &x, Order::Three).unwrap();
            let m = x.len() + 2;
            let at: Vec<f64> = x.iter().copied().chain([t, 1.0 / rho]).collect();
            let mut checked = 0;
            for k in 1..=3 {
                let mut tuples: Vec<Vec<usize>> = vec![vec![]];
                for _ in 0..k {
                    tuples = tuples.iter().flat_map(|v| (0..m).map(move |i| [&v[..], &[i]].concat())).collect();
                }
                tuples.retain(|v| v.windows(2).all(|w| w[0] <= w[1]));
                let values: Vec<(Vec<usize>, f64, f64)> = tuples
                    .into_iter()
                    .map(|vars| {
                        let args: Vec<D3> =
                            (0..m).map(|a| seeded(at[a], [0, 1, 2].map(|s| vars.get(s) == Some(&a)))).collect();
                        let want = part(&psi(&index, &args[..x.len()], args[x.len()], args[x.len() + 1]), k);
                        let idx: Vec<Var> = vars
                            .iter()
                            .map(|&a| match a {
                                a if a < x.len() => Var::N(a),
                                a if a == x.len() => Var::T,
                                _ => Var::V,
                            })
                            .collect();
                        (vars, c.psi(&idx), want)
                    })
                    .collect();
                let largest = values.iter().fold(0.0_f64, |l, (_, got, _)| l.max(got.abs()));
                for (vars, got, want) in values {
                    assert!(
                        (got - want).abs() <= 1e-12 * got.abs().max(1e-3 * largest),
                        "{t} K: {vars:?}: {got} against {want}"
                    );
                    checked += 1;
                }
            }
            assert_eq!(checked, 7 + 28 + 84);
        }
    }
}
