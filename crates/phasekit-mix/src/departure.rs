//! A binary pair's departure function α^r_ij(τ, δ) in one kernel over `Real` (f64, and `Jet4` for its τ, δ derivatives
//! to order 4; ROT-218's pattern), its terms as CoolProp 8.0.0 evaluates them (`ExcessHEFunction.h:104-196`,
//! `Helmholtz.h:377-457`; PLAN.md M13.3):
//! - Exponential: Σ n δ^d τ^t exp(−δ^l), without the exponential where l = 0;
//! - GERG-2008: `power` terms n δ^d τ^t, then n δ^d τ^t exp(−η(δ − ε)² − β(δ − γ)): β multiplies δ − γ, not its square;
//! - Gaussian + Exponential: `power` terms n δ^d τ^t exp(−δ^l), then n δ^d τ^t exp(−η(δ − ε)² − β(τ − γ)²).

use phasekit_core::{Derivs, Jet4, Order, Real};

use crate::data::{DepartureData, Kind, Term};

/// α^r_ij at (τ, δ).
pub(crate) fn alpha<R: Real>(f: &DepartureData, tau: R, delta: R) -> R {
    let mut sum = R::from_f64(0.0);
    for (k, term) in f.terms.iter().enumerate() {
        sum = sum + one(f.kind, k >= f.power, term, tau, delta);
    }
    sum
}

/// One term: its power part times its exponential, if it has one.
fn one<R: Real>(kind: Kind, gaussian: bool, term: &Term, tau: R, delta: R) -> R {
    let power = R::from_f64(term.n) * delta.powi(term.d as i32) * tau.powf(term.t);
    let c = |x: f64| R::from_f64(x);
    let exponent = match (kind, gaussian) {
        (Kind::Gerg2008, false) => return power,
        (Kind::Exponential, _) | (Kind::GaussianExponential, false) if term.l == 0.0 => return power,
        (Kind::Exponential, _) | (Kind::GaussianExponential, false) => c(0.0) - delta.powi(term.l as i32),
        (Kind::Gerg2008, true) => {
            let shifted = delta - c(term.epsilon);
            c(0.0) - c(term.eta) * shifted * shifted - c(term.beta) * (delta - c(term.gamma))
        }
        (Kind::GaussianExponential, true) => {
            let (d, t) = (delta - c(term.epsilon), tau - c(term.gamma));
            c(0.0) - c(term.eta) * d * d - c(term.beta) * t * t
        }
    };
    power * exponent.exp()
}

/// α^r_ij's scaled derivatives A_ij = τ^i δ^j ∂^(i+j)α^r_ij/∂τ^i∂δ^j at (τ, δ), to `order`.
pub(crate) fn derivs(f: &DepartureData, tau: f64, delta: f64, order: Order) -> Derivs {
    let jet = alpha(f, Jet4::tau(tau), Jet4::delta(delta)).derivs(tau, delta);
    Derivs::from_fn(order, |i, j| jet.get(i, j).unwrap_or(f64::NAN))
}

#[cfg(test)]
mod tests {
    use num_dual::{DualNum, HyperDual};
    use phasekit_core::{Order, math};

    use super::*;
    use crate::generated::DEPARTURES;

    /// Four ε directions (outer: 1 and 2, inner: 3 and 4): one evaluation gives one mixed derivative up to order 4.
    type D4 = HyperDual<HyperDual<f64>>;

    fn var(x: f64, seeds: [bool; 4]) -> D4 {
        let one = |seeded: bool| if seeded { 1.0 } else { 0.0 };
        let constant = |v: f64| HyperDual::new(v, 0.0, 0.0, 0.0);
        let inner = HyperDual::new(x, one(seeds[2]), one(seeds[3]), 0.0);
        HyperDual::new(inner, constant(one(seeds[0])), constant(one(seeds[1])), constant(0.0))
    }

    fn part(f: &D4, n: usize) -> f64 {
        let outer = match n {
            0 => &f.re,
            1 => &f.eps1,
            _ => &f.eps1eps2,
        };
        match n {
            0..=2 => outer.re,
            3 => outer.eps1,
            _ => outer.eps1eps2,
        }
    }

    /// A departure function's terms, written again on num-dual from the formulas of `ExcessHEFunction.h`.
    fn reference(f: &DepartureData, tau: &D4, delta: &D4) -> D4 {
        let mut sum = D4::from(0.0);
        for (k, t) in f.terms.iter().enumerate() {
            let power = delta.powi(t.d as i32) * tau.powf(t.t) * t.n;
            let exponent = match (f.kind, k >= f.power) {
                (Kind::Gerg2008, false) => D4::from(0.0),
                (Kind::Exponential, _) | (Kind::GaussianExponential, false) => {
                    if t.l == 0.0 {
                        D4::from(0.0)
                    } else {
                        -delta.powi(t.l as i32)
                    }
                }
                (Kind::Gerg2008, true) => -((*delta - t.epsilon).powi(2) * t.eta) - (*delta - t.gamma) * t.beta,
                (Kind::GaussianExponential, true) => {
                    -((*delta - t.epsilon).powi(2) * t.eta) - (*tau - t.gamma).powi(2) * t.beta
                }
            };
            sum += power * exponent.exp();
        }
        sum
    }

    /// ROT-218's pattern: every departure function's A_ij to order 4, from the one kernel on `Jet4`, against num-dual at
    /// three states, within 1e-12 of the larger of |A_ij| and 1.
    #[test]
    fn departure_kinds_match_ad() {
        let seeds = |lo: usize, hi: usize| [0, 1, 2, 3].map(|k| (lo..hi).contains(&k));
        for f in &DEPARTURES {
            for (tau, delta) in [(0.8, 0.8), (1.25, 1.6), (0.5, 0.2)] {
                let d = derivs(f, tau, delta, Order::Four);
                for (i, j) in (0..=4).flat_map(|n| (0..=n).map(move |i| (i, n - i))) {
                    let ad = reference(f, &var(tau, seeds(0, i)), &var(delta, seeds(i, i + j)));
                    let want = part(&ad, i + j) * math::powi(tau, i as i32) * math::powi(delta, j as i32);
                    let got = d.get(i, j).unwrap();
                    assert!(
                        (got - want).abs() <= 1e-12 * want.abs().max(1.0),
                        "{} A{i}{j}: {got} against {want}",
                        f.name
                    );
                }
            }
        }
    }

    /// The f64 kernel is the jet's value, bit for bit, so α^r_ij evaluated alone costs no derivatives.
    #[test]
    fn the_value_is_the_jet_s() {
        for f in &DEPARTURES {
            let jet = derivs(f, 0.9, 1.1, Order::One);
            assert_eq!(alpha(f, 0.9, 1.1).to_bits(), jet.get(0, 0).unwrap().to_bits(), "{}", f.name);
        }
    }
}
