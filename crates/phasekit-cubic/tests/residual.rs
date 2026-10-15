//! The generalised cubic's residual (PLAN.md M11.2): its critical conditions (ROT-217), its jets against AD (ROT-218),
//! finite differences and its exact virials.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use num_dual::{DualNum, HyperDual};
use phasekit_core::{DataSet, Derivs, HelmholtzModel, Order, Real, Virials, math};
use phasekit_cubic::{Alpha, Kind, R_CODATA, Residual};
use phasekit_verify::conformance::fd_first_order;

const KINDS: [Kind; 3] = [Kind::Vdw, Kind::Srk, Kind::Pr];

/// n-Propane-like scales: T_c 369.89 K, p_c 4.2512 MPa.
const T_C: f64 = 369.89;
const P_C: f64 = 4.2512e6;

fn residual(kind: Kind) -> Residual {
    Residual::new(kind, DataSet::Corrected, Alpha::One, T_C, P_C)
}

/// ROT-217, map 06 §8: at each kind's critical point (T_c, ρ_c = p_c/(Z_c R T_c)), ∂p/∂ρ and ∂²p/∂ρ² vanish and
/// Z_c is 3/8, 1/3 and 0.30740 (PR): p = ρRT(1 + A01) gives ∂p/∂ρ = RT(1 + 2A01 + A02) and
/// ∂²p/∂ρ² = RT/ρ (2A01 + 4A02 + A03). The constants are the closed forms: 27/64 and 1/8, 1/(9(2^(1/3) − 1)) and
/// (2^(1/3) − 1)/3, and PR's from η_c, the root of 3η³ + 3η² + 3η − 1.
#[test]
fn critical_point_has_zero_dp_drho() {
    for (kind, printed) in KINDS.into_iter().zip([0.375, 1.0 / 3.0, 0.30740]) {
        let rho_c = P_C / (kind.z_c() * R_CODATA * T_C);
        let d = residual(kind).derivs(T_C, rho_c, Order::Three);
        let a = |j| d.get(0, j).unwrap();
        let z = 1.0 + a(1);
        assert!((z - kind.z_c()).abs() <= 4.0 * f64::EPSILON, "{kind:?}: Z_c {z}");
        assert!((z - printed).abs() <= 5e-6, "{kind:?}: Z_c {z}, map 06 §8 {printed}");
        let (dp, d2p) = (1.0 + 2.0 * a(1) + a(2), 2.0 * a(1) + 4.0 * a(2) + a(3));
        assert!(dp.abs() <= 1e-14 && d2p.abs() <= 1e-13, "{kind:?}: {dp:e}, {d2p:e}");
    }
    let cbrt2 = Real::powf(2.0_f64, 1.0 / 3.0);
    let omegas = |kind: Kind| kind.omegas(DataSet::Corrected);
    let eta_pr = pr_eta_c();
    for (kind, a, b) in [
        (Kind::Vdw, 27.0 / 64.0, 1.0 / 8.0),
        (Kind::Srk, 1.0 / (9.0 * (cbrt2 - 1.0)), (cbrt2 - 1.0) / 3.0),
        (Kind::Pr, pr_omega_a(eta_pr), eta_pr * Kind::Pr.z_c()),
    ] {
        let o = omegas(kind);
        assert!((o.a / a - 1.0).abs() <= 4.0 * f64::EPSILON && (o.b / b - 1.0).abs() <= 4.0 * f64::EPSILON, "{kind:?}");
    }
}

/// PR's η_c = b/v_c, the root of 3η³ + 3η² + 3η − 1 in (0, 1), by Newton from 1/4.
fn pr_eta_c() -> f64 {
    let mut eta = 0.25;
    for _ in 0..50 {
        eta -= (((3.0 * eta + 3.0) * eta + 3.0) * eta - 1.0) / ((9.0 * eta + 6.0) * eta + 3.0);
    }
    eta
}

/// PR's Ω_a from η_c: Z_c = 1/(1 − η_c) − Ω_a/(Ω_b) · η_c/((1 + Δ₁η_c)(1 + Δ₂η_c)), so
/// Ω_a = Ω_b (1/(1 − η_c) − Z_c)(1 + 2η_c − η_c²)/η_c with Ω_b = η_c Z_c and Z_c from ∂p/∂v = 0.
fn pr_omega_a(eta: f64) -> f64 {
    let z_c = Kind::Pr.z_c();
    let omega_b = eta * z_c;
    omega_b * (1.0 / (1.0 - eta) - z_c) * (1.0 + 2.0 * eta - eta * eta) / eta
}

/// DIV-0047: `Parity` keeps CoolProp 8.0.0's literals, so its SRK critical point misses ∂p/∂ρ = 0 by about 1e-9 and
/// its constants differ from the closed forms by the entry's relative amounts.
#[test]
fn parity_keeps_coolprops_literals() {
    let rel = |kind: Kind| {
        let (p, c) = (kind.omegas(DataSet::Parity), kind.omegas(DataSet::Corrected));
        (p.a / c.a - 1.0, p.b / c.b - 1.0)
    };
    let (da, db) = rel(Kind::Srk);
    assert!((da + 4.36e-10).abs() < 5e-13 && (db - 3.64e-10).abs() < 5e-13, "{da:e} {db:e}");
    let (da, db) = rel(Kind::Pr);
    assert!(da == 0.0 && (db + 5.0e-14).abs() < 5e-16, "{da:e} {db:e}");
    assert_eq!(rel(Kind::Vdw), (0.0, 0.0));
    let srk = Residual::new(Kind::Srk, DataSet::Parity, Alpha::One, T_C, P_C);
    let d = srk.derivs(T_C, P_C / (Kind::Srk.z_c() * R_CODATA * T_C), Order::Two);
    let dp = 1.0 + 2.0 * d.get(0, 1).unwrap() + d.get(0, 2).unwrap();
    assert!(dp.abs() > 1e-11 && dp.abs() < 1e-8, "{dp:e}");
}

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

/// The paper formula on num-dual: α^r(τ, δ) = −ln(1 − η) − (Ω_a/Ω_b) τ φ(η), η = Ω_b δ (α = 1).
fn alphar_ad(kind: Kind, tau: &D4, delta: &D4) -> D4 {
    let o = kind.omegas(DataSet::Corrected);
    let eta = *delta * o.b;
    let phi = match kind.deltas() {
        (d1, d2) if d1 == d2 => eta,
        (d1, d2) => ((eta * d1).ln_1p() - (eta * d2).ln_1p()) / (d1 - d2),
    };
    -(-eta).ln_1p() - *tau * phi * (o.a / o.b)
}

/// ROT-218, AD oracle num-dual 0.15: every A_ij to order 4 of each kind's α^r from `Jet4` against num-dual's
/// derivatives of the formula, at 36 states from the dilute gas to the liquid, both sides of T_c, class `Term`-like
/// (relative 1e-12 of the largest of |A_ij| and 1).
#[test]
fn cubic_jets_match_ad() {
    for kind in KINDS {
        let r = residual(kind);
        for t in [0.5 * T_C, 0.8 * T_C, T_C, 1.2 * T_C, 2.0 * T_C, 4.0 * T_C] {
            for frac in [1e-6, 1e-3, 0.1, 0.5, 0.8, 0.95] {
                let rho = frac * r.rho_max();
                let d = r.derivs(t, rho, Order::Four);
                let (tau, delta) = (T_C / t, rho * R_CODATA * T_C / P_C);
                for n in 0..=4 {
                    for i in 0..=n {
                        let j = n - i;
                        let seeds = |lo: usize, hi: usize| [0, 1, 2, 3].map(|k| (lo..hi).contains(&k));
                        let f = alphar_ad(kind, &var(tau, seeds(0, i)), &var(delta, seeds(i, i + j)));
                        let want = part(&f, n) * math::powi(tau, i as i32) * math::powi(delta, j as i32);
                        let got = d.get(i, j).unwrap();
                        assert!(
                            (got - want).abs() <= 1e-12 * want.abs().max(1.0),
                            "{kind:?} T {t} rho {rho}: A{i}{j} {got} against {want}"
                        );
                    }
                }
            }
        }
    }
}

/// The residual with a monatomic ideal gas, a `HelmholtzModel` for the conformance kit.
#[derive(Debug)]
struct Model(Residual);

impl HelmholtzModel for Model {
    fn gas_constant(&self) -> f64 {
        R_CODATA
    }
    fn residual(&self, t: f64, rho: f64, order: Order) -> Derivs {
        self.0.derivs(t, rho, order)
    }
    fn ideal(&self, t: f64, rho: f64, order: Order) -> Derivs {
        Derivs::from_fn(order, |i, j| match (i, j) {
            (0, 0) => math::ln(rho) - 1.5 * math::ln(t),
            (1, 0) => 1.5,
            (2, 0) => -1.5,
            (3, 0) => 3.0,
            (4, 0) => -9.0,
            (0, j) => Derivs::IDEAL_DELTA.get(0, j).unwrap_or(0.0),
            _ => 0.0,
        })
    }
    fn rho_max(&self, _t: f64) -> f64 {
        self.0.rho_max()
    }
    fn zero_density(&self, t: f64) -> Option<Virials> {
        Some(self.0.virials(t))
    }
}

/// VERIFICATION.md §10: A10 and A01 against central differences in dimensional T and ρ.
#[test]
fn residual_matches_finite_differences() {
    for kind in KINDS {
        let model = Model(residual(kind));
        for (t, frac) in [(0.7 * T_C, 0.6), (T_C, 0.3), (1.5 * T_C, 0.05), (3.0 * T_C, 0.7)] {
            let rho = frac * model.0.rho_max();
            fd_first_order(&model, t, rho, 1e-6, 1e-7).unwrap_or_else(|e| panic!("{kind:?} {t} {rho}: {e:?}"));
        }
    }
}

/// E4: `virials` is α^r's exact expansion. A01/ρ = B + Cρ + O(ρ²), so 2f(h) − f(2h) → B and (f(2h) − f(h))/h → C;
/// dB/dT against central differences.
#[test]
fn virials_are_exact() {
    for kind in KINDS {
        let r = residual(kind);
        for t in [0.6 * T_C, T_C, 2.0 * T_C] {
            let v = r.virials(t);
            let h = 1e-6 * r.rho_max();
            let f = |rho: f64| r.derivs(t, rho, Order::One).get(0, 1).unwrap() / rho;
            let (b, c) = (2.0 * f(h) - f(2.0 * h), (f(2.0 * h) - f(h)) / h);
            assert!((b / v.b - 1.0).abs() <= 1e-9, "{kind:?} {t}: B {b} against {}", v.b);
            assert!((c / v.c - 1.0).abs() <= 1e-4, "{kind:?} {t}: C {c} against {}", v.c);
            let ht = 1e-4 * t;
            let db_dt = (r.virials(t + ht).b - r.virials(t - ht).b) / (2.0 * ht);
            assert!((db_dt / v.db_dt - 1.0).abs() <= 1e-7, "{kind:?} {t}: dB/dT {db_dt} against {}", v.db_dt);
        }
    }
}
