//! The superancillary's private parts (PLAN.md M11.5): its piece lookup on the pieces' bounds, T̃'s inverse over its
//! bracket, and the Illinois solve behind `at_p`.

use phasekit_core::{DataSet, math};

use super::*;
use crate::Alpha;

/// SRK and PR n-Propane with Soave's alpha.
fn curves() -> [CubicSaturation; 2] {
    [Kind::Srk, Kind::Pr].map(|kind| {
        let alpha = Alpha::Soave(kind.soave_m(0.1521).unwrap());
        CubicSaturation::new(Residual::new(kind, DataSet::Corrected, alpha, 369.89, 4.2512e6)).unwrap()
    })
}

/// CoolProp's `SuperAncillary::get_index` puts a T̃ on the bound between two pieces in the upper one; so does the
/// lookup, on every inner bound of the six expansions, where each piece's xmax is the next one's xmin.
#[test]
fn a_bound_belongs_to_the_upper_piece() {
    let all = [tables::SRK_P, tables::SRK_RHOL, tables::SRK_RHOV, tables::PR_P, tables::PR_RHOL, tables::PR_RHOV];
    for pieces in all {
        for (below, above) in pieces.iter().zip(pieces.iter().skip(1)) {
            let x = above.xmin;
            assert_eq!(below.xmax, x);
            assert_eq!(expansion(pieces, x).map(f64::to_bits), Some(above.eval(x).to_bits()), "{x}");
        }
    }
}

/// `t_at` inverts T̃ over its bracket [T_c/1000, 1.1 T_c] and refuses outside it: at T̃(1.05 T_c) it gives that T
/// within an ulp, and none for half T̃(T_c/1000) or 1.01 T̃(1.1 T_c).
#[test]
fn t_at_inverts_t_tilde_over_its_bracket() {
    for curve in curves() {
        let t_c = curve.residual.t_c();
        let t = 1.05 * t_c;
        let back = curve.t_at(curve.t_tilde(t)).unwrap();
        assert!((back - t).abs() <= f64::EPSILON * t, "{back} for {t}");
        assert_eq!(curve.t_at(0.5 * curve.t_tilde(1e-3 * t_c)), None);
        assert_eq!(curve.t_at(1.01 * curve.t_tilde(1.1 * t_c)), None);
    }
}

/// The solve behind `at_p` meets each of 40 pressures across each curve's range within a few ulp of its T in at most 24
/// evaluations of ln p_sat, its two ends' included (19 at most here, 2 at the ends, where an end's value is 0); halving
/// T's range to adjacent doubles takes 52 more.
#[test]
fn the_solve_takes_few_evaluations() {
    for curve in curves() {
        let (lo, hi) = curve.t_range;
        for k in 0..40 {
            let t = lo + (hi - lo) * f64::from(k) / 39.0;
            let target = math::ln(curve.at_t(t).unwrap().bubble.p);
            let mut calls = 0;
            let ln_p = |t: f64| {
                calls += 1;
                curve.at_t(t).map(|s| math::ln(s.bubble.p) - target)
            };
            let root = bracketed_root(ln_p, curve.t_range).unwrap();
            assert!((root - t).abs() <= 4.0 * f64::EPSILON * t && calls <= 24, "{root} for {t} in {calls}");
        }
    }
}

/// The solve takes √2 and √3 from x² − 2 and x² − 3 on [1, 2] within an ulp in at most 14 evaluations (11 and 10).
#[test]
fn the_solve_takes_square_roots() {
    for (c, root) in [(2.0, core::f64::consts::SQRT_2), (3.0, 1.732_050_807_568_877_2)] {
        let mut calls = 0;
        let f = |x: f64| {
            calls += 1;
            Ok(x * x - c)
        };
        let found = bracketed_root(f, (1.0, 2.0)).unwrap();
        assert!((found - root).abs() <= f64::EPSILON * root && calls <= 14, "{found} for {root} in {calls}");
    }
}

/// Once the ends are adjacent doubles the root is the end with the smaller residual, the lower end on a tie.
#[test]
fn adjacent_ends_give_the_smaller_residual() {
    let (x, y) = (1.0, 1.0_f64.next_up());
    for ((fa, fb), root) in [((1.0, -0.5), y), ((0.5, -1.0), x), ((0.5, -0.5), x)] {
        assert_eq!(Bracket { a: (x, fa), b: (y, fb), side: 0 }.root(), Some(root), "{fa} {fb}");
    }
}

/// An end whose value is exactly 0 is the root before any step: x − 0.1 on [0.1, 0.5] and x − 0.9 on [0.3, 0.9], where
/// false position would land one ulp inside, 0.10000000000000002 and 0.9000000000000001.
#[test]
fn a_zero_end_is_the_root() {
    for (zero, ends) in [(0.1, (0.1, 0.5)), (0.9, (0.3, 0.9))] {
        let mut calls = 0;
        let f = |x: f64| {
            calls += 1;
            Ok(x - zero)
        };
        assert_eq!(bracketed_root(f, ends).unwrap(), zero);
        assert_eq!(calls, 2);
    }
}

/// The Illinois rule: an end that moves twice running halves the other end's value, and only then.
#[test]
fn a_second_move_running_halves_the_other_value() {
    let mut bracket = Bracket { a: (0.0, -1.0), b: (1.0, 1.0), side: 0 };
    let steps = [
        ((0.2, -0.5), ((0.2, -0.5), (1.0, 1.0))),
        ((0.4, -0.25), ((0.4, -0.25), (1.0, 0.5))),
        ((0.8, 0.5), ((0.4, -0.25), (0.8, 0.5))),
        ((0.6, 0.25), ((0.4, -0.125), (0.6, 0.25))),
        ((0.5, -0.125), ((0.5, -0.125), (0.6, 0.25))),
    ];
    for ((c, fc), ends) in steps {
        bracket.update(c, fc);
        assert_eq!((bracket.a, bracket.b), ends, "after {c}");
    }
}

/// Where false position lands on an end, the solve bisects instead: a step at 1.5 from 1e-300 to −1, or from 1 to
/// −1e-300, whose smaller side is the double next to 1.5 on the side of the 1e-300.
#[test]
fn a_step_on_an_end_bisects() {
    let below = |x: f64| Ok(if x < 1.5 { 1e-300 } else { -1.0 });
    let above = |x: f64| Ok(if x <= 1.5 { 1.0 } else { -1e-300 });
    assert_eq!(bracketed_root(below, (1.0, 2.0)).unwrap(), 1.499_999_999_999_999_8);
    assert_eq!(bracketed_root(above, (1.0, 2.0)).unwrap(), 1.500_000_000_000_000_2);
}
