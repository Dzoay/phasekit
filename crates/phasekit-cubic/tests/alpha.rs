//! The alpha functions (PLAN.md M11.3): α(θ) and its T-derivatives to order 4 against AD, α(T_c) = 1, Soave's m(ω),
//! and the residual and virials with each alpha.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use num_dual::{DualNum, HyperDual};
use phasekit_core::{DataSet, Jet4, Order, math};
use phasekit_cubic::{Alpha, Kind, R_CODATA, Residual};

/// A nested hyper-dual with four ε directions, as in `phasekit-verify`'s `tests/terms.rs`.
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

/// The forms as the papers write them, on num-dual (map 06 §3.1).
fn alpha_ad(alpha: Alpha, theta: &D4) -> D4 {
    let d = -theta.sqrt() + 1.0;
    match alpha {
        Alpha::One => *theta * 0.0 + 1.0,
        Alpha::Soave(m) => (d * m + 1.0).powi(2),
        Alpha::MathiasCopeman([c1, c2, c3]) => (d * c1 + d.powi(2) * c2 + d.powi(3) * c3 + 1.0).powi(2),
        Alpha::Twu([l, m, n]) => theta.powf(n * (m - 1.0)) * ((-theta.powf(m * n) + 1.0) * l).exp(),
        _ => unreachable!(),
    }
}

const FORMS: [Alpha; 4] =
    [Alpha::One, Alpha::Soave(0.6), Alpha::MathiasCopeman([0.7, -0.3, 0.2]), Alpha::Twu([0.3, 0.9, 2.0])];

/// AD oracle num-dual 0.15: each form's α and its first four θ-derivatives from `Jet4` against num-dual's, below and
/// above T_c (relative 1e-13 of the largest of |value| and 1); and α(1) = 1.
#[test]
fn alpha_derivatives_match_ad() {
    for alpha in FORMS {
        assert!((alpha.eval(1.0_f64) - 1.0).abs() <= f64::EPSILON, "{alpha:?}: α(T_c) {}", alpha.eval(1.0_f64));
        for theta in [0.3, 0.7, 1.0, 1.4, 3.0] {
            let jet = alpha.eval(Jet4::tau(theta));
            for n in 0..=4 {
                let seeds = [0, 1, 2, 3].map(|k| k < n);
                let want = part(&alpha_ad(alpha, &var(theta, seeds)), n);
                let got = jet.derivative(n, 0).unwrap();
                assert!((got - want).abs() <= 1e-13 * want.abs().max(1.0), "{alpha:?} θ {theta}: d{n} {got} {want}");
            }
        }
    }
}

/// Soave's m(ω) for n-Propane's ω = 0.1521, in exact decimals: SRK 0.480 + 1.574ω − 0.176ω² = 0.71533374384,
/// PR 0.37464 + 1.54226ω − 0.26992ω² = 0.6029733060528; van der Waals has none.
#[test]
fn soave_polynomials() {
    let close = |got: Option<f64>, want: f64| got.is_some_and(|m| (m - want).abs() <= 1e-15);
    assert!(close(Kind::Srk.soave_m(0.1521), 0.715_333_743_84), "{:?}", Kind::Srk.soave_m(0.1521));
    assert!(close(Kind::Pr.soave_m(0.1521), 0.602_973_306_052_8), "{:?}", Kind::Pr.soave_m(0.1521));
    assert_eq!(Kind::Vdw.soave_m(0.1521), None);
    assert_eq!((Kind::Srk.soave_m(0.0), Kind::Pr.soave_m(0.0)), (Some(0.480), Some(0.37464)));
}

/// The PR residual with each alpha against num-dual's α^r(τ, δ) = −ln(1 − η) − (Ω_a/Ω_b) α(1/τ) τ φ(η), every A_ij to
/// order 4; and the virials' dB/dT, which carries da/dT, against central differences.
#[test]
fn residual_with_each_alpha_matches_ad() {
    let (t_c, p_c) = (369.89, 4.2512e6);
    let o = Kind::Pr.omegas(DataSet::Corrected);
    let (d1, d2) = Kind::Pr.deltas();
    for alpha in FORMS {
        let r = Residual::new(Kind::Pr, DataSet::Corrected, alpha, t_c, p_c);
        for (t, frac) in [(0.6 * t_c, 0.7), (t_c, 0.3), (2.0 * t_c, 0.05)] {
            let rho = frac * r.rho_max();
            let d = r.derivs(t, rho, Order::Four);
            let (tau, delta) = (t_c / t, rho * R_CODATA * t_c / p_c);
            for n in 0..=4 {
                for i in 0..=n {
                    let j = n - i;
                    let seeds = |lo: usize, hi: usize| [0, 1, 2, 3].map(|k| (lo..hi).contains(&k));
                    let (tt, dd) = (var(tau, seeds(0, i)), var(delta, seeds(i, i + j)));
                    let eta = dd * o.b;
                    let phi = ((eta * d1).ln_1p() - (eta * d2).ln_1p()) / (d1 - d2);
                    let theta = tt.recip();
                    let f = -(-eta).ln_1p() - alpha_ad(alpha, &theta) * tt * phi * (o.a / o.b);
                    let want = part(&f, n) * math::powi(tau, i as i32) * math::powi(delta, j as i32);
                    let got = d.get(i, j).unwrap();
                    assert!((got - want).abs() <= 1e-12 * want.abs().max(1.0), "{alpha:?} T {t}: A{i}{j} {got} {want}");
                }
            }
            let ht = 1e-4 * t;
            let db_dt = (r.virials(t + ht).b - r.virials(t - ht).b) / (2.0 * ht);
            let v = r.virials(t);
            assert!((db_dt / v.db_dt - 1.0).abs() <= 1e-7, "{alpha:?} {t}: dB/dT {db_dt} against {}", v.db_dt);
        }
    }
}
