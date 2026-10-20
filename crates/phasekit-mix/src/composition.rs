//! A multi-fluid model's composition derivatives (PLAN.md M13.6; ROT-191, ROT-210): one mole-number AD path.
//!
//! Ψ(T, V, n) = A^r/(RT) = n α^r(τ, δ, x), with n = Σ n_i, x = n/n, τ = T_r(x)/T and δ = n v_r(x)/V, is evaluated on
//! [`Taylor`] numbers in the variables (n_1 ... n_N, T, V) at one mole of composition x and V = 1/ρ. Each component's
//! and each departure function's (τ, δ) jet to order 3 is composed with τ(n) and δ(n), which share their powers, so
//! every derivative of Ψ to order 3 comes from one evaluation, with no hand-written composition derivative (CoolProp's
//! double-count its ideal term and miss a factor of 2, ROT-191, ROT-210). Fugacities, their derivatives and partial
//! molar volumes follow from Ψ's derivatives by Michelsen and Mollerup's relations.

use phasekit_core::batch::{ExecPolicy, RealKernel, run_kernel};
use phasekit_core::{Constant, Derivs, Error, HelmholtzModel, Order, Prop, Real};

use crate::MultiFluid;
use crate::data::DepartureData;
use crate::departure::{self, Jet};
use crate::multifluid::Component;
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
    let f = |k: usize| match k {
        0 | 1 => 1.0,
        2 => 2.0,
        _ => 6.0,
    };
    1.0 / (f(a) * f(b))
}

/// Δτ^a Δδ^b for a + b ≤ `k`, shared by every part: row a holds b = 0 ..= k − a, from `one` and the increments
/// (Δτ, Δδ).
#[allow(clippy::arithmetic_side_effects, reason = "multivariate series (&Taylor): IEEE arithmetic, no panic")]
fn increments<R: Real>(one: &Taylor<R>, k: usize, (dt, dd): (&Taylor<R>, &Taylor<R>)) -> Vec<Vec<Taylor<R>>> {
    let mut powers = vec![vec![one.clone()]];
    for _ in 1..=k {
        let Some(last) = powers.last().and_then(|row| row.first()) else { break };
        let next = last * dt;
        powers.push(vec![next]);
    }
    for (a, row) in powers.iter_mut().enumerate() {
        for _ in 0..k.saturating_sub(a) {
            let Some(last) = row.last() else { break };
            let next = last * dd;
            row.push(next);
        }
    }
    powers
}

/// Ψ's variables (n_1 ... n_N, T, V) on Taylor numbers of order `k` at one mole of composition x and V = 1/ρ, with the
/// total n, the fractions and τ and δ of them: what every part of Ψ is composed with, on `f64` or on lanes.
struct Reduced<R> {
    total: Taylor<R>,
    fractions: Vec<Taylor<R>>,
    tau: Taylor<R>,
    delta: Taylor<R>,
}

/// [`Reduced`] of mole fractions `x` at (T, ρ) to order `k`, `present` as `reduce_taylor` takes it. Each value is
/// computed from values alone, so order 0's are every order's, where they are finite.
#[allow(clippy::arithmetic_side_effects, reason = "multivariate series (&Taylor): IEEE arithmetic, no panic")]
fn reduced<R: Real>(
    model: &MultiFluid,
    x: &[R],
    (t, rho): (R, R),
    k: usize,
    present: impl Fn(&Taylor<R>) -> bool,
) -> Reduced<R> {
    let n = x.len();
    let m = n.saturating_add(2);
    let moles: Vec<Taylor<R>> = x.iter().enumerate().map(|(i, xi)| Taylor::variable(m, k, i, *xi)).collect();
    let (temperature, volume) =
        (Taylor::variable(m, k, n, t), Taylor::variable(m, k, n.saturating_add(1), R::from_f64(1.0) / rho));
    let mut total = Taylor::constant(m, k, R::from_f64(0.0));
    for ni in &moles {
        total.add_scaled(R::from_f64(1.0), ni);
    }
    let inverse = total.recip();
    let fractions: Vec<Taylor<R>> = moles.iter().map(|ni| ni * &inverse).collect();
    let [t_r, v_r] = model.reducing.reduce_taylor(&fractions, present);
    let tau = t_r.divide(&temperature);
    let delta = (&total * &v_r).divide(&volume);
    Reduced { total, fractions, tau, delta }
}

/// Ψ = n α^r on `r`'s variables to order `k`: each component's (τ, δ) jet, from its derivatives `component(i, a, b)`
/// at the mixture's τ and δ (as `Derivs::get` gives them), and each departure function's from `departure_jet`,
/// composed with τ(n) and δ(n). The scalar path passes `departure::jet`, which keeps the jet's code out of line, as it
/// was before Ψ became generic: inlined, it made `composition` half as large again and a QT flash up to 8 % slower.
#[allow(clippy::arithmetic_side_effects, reason = "multivariate series (&Taylor): IEEE arithmetic, no panic")]
fn psi<R: Real>(
    model: &MultiFluid,
    r: &Reduced<R>,
    k: usize,
    component: impl Fn(usize, usize, usize) -> R,
    departure_jet: impl Fn(&DepartureData, R, R, usize) -> Jet<R>,
) -> Result<Taylor<R>, Error> {
    let m = r.total.vars();
    let (tau0, delta0) = (r.tau.value(), r.delta.value());
    let powers = increments(&Taylor::constant(m, k, R::from_f64(1.0)), k, (&r.tau.increment(), &r.delta.increment()));
    let compose = |jet: &dyn Fn(usize, usize) -> R| {
        let mut sum = Taylor::constant(m, k, R::from_f64(0.0));
        for (a, row) in powers.iter().enumerate() {
            for (b, power) in row.iter().enumerate() {
                sum.add_scaled(jet(a, b) * inverse_factorials(a, b), power);
            }
        }
        sum
    };
    let mut alpha = Taylor::constant(m, k, R::from_f64(0.0));
    for (i, xi) in r.fractions.iter().enumerate() {
        // The component's scaled derivatives at the mixture's τ and δ, unscaled.
        let jet = |a: usize, b: usize| component(i, a, b) / (tau0.powi(a as i32) * delta0.powi(b as i32));
        alpha = &alpha + &(xi * &compose(&jet));
    }
    for d in model.departures.iter() {
        let jet = departure_jet(d.function, tau0, delta0, k);
        let part = compose(&|a, b| jet.derivative(a, b).unwrap_or(R::from_f64(f64::NAN)));
        let absent = || Error::InvalidInput { quantity: "a departure's component", value: d.i as f64 };
        let (xi, xj) = (r.fractions.get(d.i).ok_or_else(absent)?, r.fractions.get(d.j).ok_or_else(absent)?);
        alpha = &alpha + &(&(xi * xj) * &part.scale(R::from_f64(d.f)));
    }
    Ok(&r.total * &alpha)
}

/// A component's Helmholtz model.
fn helmholtz(c: &Component) -> Result<&dyn HelmholtzModel, Error> {
    c.fluid.model().helmholtz().ok_or(Error::NoModel { prop: Prop::Constant(Constant::GasConstant) })
}

impl MixtureModel for MultiFluid {
    fn components(&self) -> usize {
        self.len()
    }

    fn composition(&self, t: f64, rho: f64, x: &[f64], order: Order) -> Result<Composition, Error> {
        let k = self.composition_order(t, rho, x, order)?;
        let r = reduced(self, x, (t, rho), k, |d| d.value() != 0.0);
        let (tau0, delta0) = (r.tau.value(), r.delta.value());
        let parts = self.components.iter().map(|c| Ok(helmholtz(c)?.residual(c.t_r / tau0, delta0 * c.rho_r, order)));
        let parts = parts.collect::<Result<Vec<Derivs>, Error>>()?;
        let component = |i, a, b| parts.get(i).and_then(|d: &Derivs| d.get(a, b)).unwrap_or(f64::NAN);
        let psi = psi(self, &r, k, component, departure::jet)?;
        Ok(Composition { psi, components: self.len(), t, rho, r: self.gas_constant() })
    }
}

impl MultiFluid {
    /// The order's number, 1 to 3, where [`MixtureModel::composition`] admits its arguments: one mole fraction per
    /// component, (T, ρ) finite and positive, an order of at most 3. A caller that solves several phases' densities
    /// before their composition derivatives checks each phase with it in turn, so its refusals come in the same order.
    pub(crate) fn composition_order(&self, t: f64, rho: f64, x: &[f64], order: Order) -> Result<usize, Error> {
        if x.len() != self.len() {
            return Err(Error::InvalidInput { quantity: "number of mole fractions", value: x.len() as f64 });
        }
        if !(t > 0.0 && rho > 0.0 && t.is_finite() && rho.is_finite()) {
            return Err(Error::InvalidInput { quantity: "density for composition derivatives", value: rho });
        }
        degree(order)
    }

    /// The composition derivatives of `phases`, each a density and its mole fractions, at one temperature T, each
    /// [`MixtureModel::composition`]'s bit for bit (PLAN.md M22.5). Under `ExecPolicy::Simd` side by side: each
    /// phase's τ and δ, its components' derivatives there through each component's own `residual_batch`, then the rest
    /// of Ψ as one kernel over `Real` ([`Phases`]) through `batch::run_kernel`. Under the other policies, for a single
    /// phase, and where a fraction is not positive (the reducing functions then test a pair's denominator, which `Real`
    /// cannot), point by point. The first refusal is the first phase's that `composition` refuses.
    pub(crate) fn compositions(
        &self,
        t: f64,
        phases: &[(f64, &[f64])],
        order: Order,
        exec: ExecPolicy,
    ) -> Result<Vec<Composition>, Error> {
        let point_by_point = || phases.iter().map(|(rho, x)| self.composition(t, *rho, x, order)).collect();
        // Before the phases' τ and δ, which only the lanes use: the other policies, and one phase alone, which would
        // fill a group of lanes with copies of itself, pay nothing for them.
        // Equivalent (M22.5, M22.6): either path gives each phase's `composition` bit for bit (K17), so which one a
        // batch takes changes only the time.
        // equivalent mutant: replace != with ==
        // equivalent mutant: replace || with &&
        // equivalent mutant: replace < with ==
        // equivalent mutant: replace < with >
        // equivalent mutant: replace < with <=
        if exec != ExecPolicy::Simd || phases.len() < 2 {
            return point_by_point();
        }
        let mut k = 0;
        for (rho, x) in phases {
            k = self.composition_order(t, *rho, x, order)?;
        }
        // Equivalent (M22.5): with `==` or `<` no phase a flash passes counts as positive (its fractions sum to 1), so
        // every phase runs point by point, which gives the same bits (K17); only the lanes given a fraction at 0 differ,
        // which `two_phase_lanes_are_scalar_bitwise` catches.
        // equivalent mutant: replace > with ==
        // equivalent mutant: replace > with <
        let positive = phases.iter().all(|(_, x)| x.iter().all(|x| *x > 0.0));
        if !positive {
            return point_by_point();
        }
        let scaled: Vec<(f64, f64)> = phases
            .iter()
            .map(|(rho, x)| reduced(self, x, (t, *rho), 0, |d| d.value() != 0.0))
            .map(|r| (r.tau.value(), r.delta.value()))
            .collect();
        let inputs = self.phase_columns(t, phases, &scaled, order, exec)?;
        let points = phases.len();
        let m = self.len().saturating_add(2);
        let mut outputs = vec![vec![0.0; points]; Taylor::<f64>::len(m, k)];
        let input_refs: Vec<&[f64]> = inputs.iter().map(Vec::as_slice).collect();
        let mut output_refs: Vec<&mut [f64]> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
        run_kernel(&Phases { model: self, order, k }, &input_refs, &mut output_refs, exec)?;
        let psi =
            |p: usize| Taylor::from_coefficients(m, k, outputs.iter().map(|c| c.get(p).copied().unwrap_or(f64::NAN)));
        let r = self.gas_constant();
        Ok(phases
            .iter()
            .enumerate()
            .map(|(p, (rho, _))| Composition { psi: psi(p), components: self.len(), t, rho: *rho, r })
            .collect())
    }

    /// [`Phases`]' input columns for `phases` at T, whose τ and δ are `scaled`: each phase's mole fractions, T and ρ,
    /// then each component's derivatives at (T_r,i/τ, δ ρ_r,i) from its own `residual_batch` under `exec`.
    fn phase_columns(
        &self,
        t: f64,
        phases: &[(f64, &[f64])],
        scaled: &[(f64, f64)],
        order: Order,
        exec: ExecPolicy,
    ) -> Result<Vec<Vec<f64>>, Error> {
        let n = self.len();
        let mut columns: Vec<Vec<f64>> =
            (0..n).map(|i| phases.iter().map(|(_, x)| x.get(i).copied().unwrap_or(f64::NAN)).collect()).collect();
        columns.push(vec![t; phases.len()]);
        columns.push(phases.iter().map(|(rho, _)| *rho).collect());
        for c in self.components.iter() {
            let (ts, rhos): (Vec<f64>, Vec<f64>) =
                scaled.iter().map(|(tau, delta)| (c.t_r / tau, delta * c.rho_r)).unzip();
            let mut parts = vec![Derivs::zero(order); phases.len()];
            helmholtz(c)?.residual_batch(&ts, &rhos, order, &mut parts, exec)?;
            let entries = |d: &Derivs| departure::entries(d, order).collect::<Vec<f64>>();
            let rows: Vec<Vec<f64>> = parts.iter().map(entries).collect();
            for j in 0..departure::slots(order) {
                columns.push(rows.iter().map(|row| row.get(j).copied().unwrap_or(f64::NAN)).collect());
            }
        }
        Ok(columns)
    }
}

/// Ψ of phases at one temperature as one kernel over `Real` for core's runner (PLAN.md M22.5; user decision ML3): in, a
/// phase's mole fractions (one per component), T and ρ, then each component's derivatives at the mixture's τ and δ
/// (`slots(order)` a component, in `Derivs::from_fn`'s order); out, Ψ's coefficients to order `k`
/// ([`Taylor::coefficients`]). Its fractions are positive, so no pair's denominator is 0.
struct Phases<'a> {
    model: &'a MultiFluid,
    order: Order,
    k: usize,
}

impl RealKernel for Phases<'_> {
    fn eval<R: Real>(&self, inputs: &[R], outputs: &mut [R]) {
        let nan = R::from_f64(f64::NAN);
        let n = self.model.len();
        let input = |i: usize| inputs.get(i).copied().unwrap_or(nan);
        let x: Vec<R> = (0..n).map(input).collect();
        let (t, rho) = (input(n), input(n.saturating_add(1)));
        let s = departure::slots(self.order);
        let part = |c: usize| {
            let first = c.saturating_mul(s).saturating_add(n).saturating_add(2);
            let mut values = (first..first.saturating_add(s)).map(input);
            Derivs::from_fn(self.order, |_, _| values.next().unwrap_or(nan))
        };
        let parts: Vec<Derivs<R>> = (0..n).map(part).collect();
        let r = reduced(self.model, &x, (t, rho), self.k, |_| true);
        let component = |i, a, b| parts.get(i).and_then(|d: &Derivs<R>| d.get(a, b)).unwrap_or(nan);
        let psi = psi(self.model, &r, self.k, component, departure::jet_on);
        match psi {
            Ok(psi) => outputs.iter_mut().zip(psi.coefficients()).for_each(|(o, v)| *o = v),
            Err(_) => outputs.iter_mut().for_each(|o| *o = nan),
        }
    }
}

impl Composition {
    /// The index of `var` among Ψ's variables.
    fn index(&self, var: Var) -> usize {
        match var {
            Var::N(i) => i,
            Var::T => self.components,
            Var::V => self.components.saturating_add(1),
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

    /// Phases of `model` at T = 0.8 T_r: 17 random densities between 0.05 and 2.5 times the reducing density, each with
    /// random positive mole fractions, and one with a fraction of 0.
    fn random_phases(model: &MultiFluid, seed: u64) -> (f64, Vec<(f64, Vec<f64>)>) {
        let mut state = seed;
        let mut uniform = || {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (state >> 11) as f64 / (1_u64 << 53) as f64
        };
        let n = model.len();
        let (t_r, rho_r) = model.reducing_state(&vec![1.0 / n as f64; n]);
        let mut phases: Vec<(f64, Vec<f64>)> = (0..17)
            .map(|_| {
                let raw: Vec<f64> = (0..n).map(|_| 0.05 + uniform()).collect();
                let sum: f64 = raw.iter().sum();
                ((0.05 + 2.45 * uniform()) * rho_r, raw.iter().map(|x| x / sum).collect())
            })
            .collect();
        let mut absent = vec![0.0; n];
        absent[0] = 1.0;
        phases.push((rho_r, absent));
        (0.8 * t_r, phases)
    }

    /// Phases whose τ overflows, at T = 1e-310 (positive and finite, so `composition` admits it; τ = T_r/T = ∞), and
    /// whose δ overflows, at ρ = 1e308, are in the lanes too each phase's [`MixtureModel::composition`], every
    /// coefficient's bits (NaN).
    #[test]
    fn phases_whose_tau_or_delta_overflows_are_each_composition_s() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let model = MultiFluid::new(&registry, &["R32", "R125"], DataSet::Parity).unwrap();
        let x = [0.4, 0.6];
        let (t_r, rho_r) = model.reducing_state(&x);
        let bits = |c: Result<Vec<Composition>, Error>| {
            c.map(|c| {
                c.iter().map(|c| c.psi.coefficients().map(f64::to_bits).collect::<Vec<u64>>()).collect::<Vec<_>>()
            })
        };
        for (t, rhos) in [(1e-310, [rho_r, 0.5 * rho_r]), (t_r, [1e308, rho_r])] {
            let points: Vec<(f64, &[f64])> = rhos.iter().map(|rho| (*rho, &x[..])).collect();
            let scalar = points.iter().map(|(rho, x)| model.composition(t, *rho, x, Order::Two)).collect();
            let batch = model.compositions(t, &points, Order::Two, ExecPolicy::Simd);
            assert_eq!(bits(batch), bits(scalar), "T {t:e}, ρ {rhos:?}");
        }
    }

    /// [`two_phase_lanes_are_scalar_bitwise`]'s comparison of one batch: `points` at T through
    /// [`MultiFluid::compositions`] under `exec`, each phase's Ψ against [`MixtureModel::composition`]'s, every
    /// coefficient's bits. Returns the phases compared.
    fn assert_compositions_bitwise(
        model: &MultiFluid,
        t: f64,
        points: &[(f64, &[f64])],
        order: Order,
        exec: ExecPolicy,
    ) -> usize {
        let bits = |c: &Composition| c.psi.coefficients().map(f64::to_bits).collect::<Vec<u64>>();
        let batch = model.compositions(t, points, order, exec).unwrap();
        assert_eq!(batch.len(), points.len());
        for ((rho, x), c) in points.iter().zip(&batch) {
            let scalar = model.composition(t, *rho, x, order).unwrap();
            let count = points.len();
            assert_eq!(bits(c), bits(&scalar), "{order:?} {exec:?}, {count} phases: ρ {rho}, x {x:?}");
        }
        batch.len()
    }

    /// PLAN.md M22.5 (VERIFICATION.md §9.2, class `Exact`): the composition derivatives of phases at one temperature
    /// side by side in core's lanes are each phase's [`MixtureModel::composition`] bit for bit, every coefficient of Ψ:
    /// a library binary and ternary and a GERG-2008 binary, orders 1 to 3, the first 1, 2, 3, 5 and 17 of 17 random
    /// phases (every remainder of the lanes' groups), and all 18 with one whose fraction is 0, which runs point by
    /// point; under `Reference` too.
    #[test]
    fn two_phase_lanes_are_scalar_bitwise() {
        let registry = Registry::from_embedded(DataSet::Parity).unwrap();
        let models = [
            MultiFluid::new(&registry, &["R32", "R125"], DataSet::Parity).unwrap(),
            MultiFluid::new(&registry, &["Methane", "Ethane", "n-Propane"], DataSet::Parity).unwrap(),
            MultiFluid::gerg2008(&registry, &["Methane", "Nitrogen"]).unwrap(),
        ];
        let mut compared = 0;
        for (seed, model) in (1..).zip(&models) {
            let (t, phases) = random_phases(model, seed);
            for order in [Order::One, Order::Two, Order::Three] {
                for (count, exec) in
                    [1, 2, 3, 5, 17, 18].map(|n| (n, ExecPolicy::Simd)).into_iter().chain([(18, ExecPolicy::Reference)])
                {
                    let points: Vec<(f64, &[f64])> =
                        phases[..count].iter().map(|(rho, x)| (*rho, x.as_slice())).collect();
                    compared += assert_compositions_bitwise(model, t, &points, order, exec);
                }
            }
        }
        assert_eq!(compared, models.len() * 3 * (1 + 2 + 3 + 5 + 17 + 18 + 18));
    }
}
