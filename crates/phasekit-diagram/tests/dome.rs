//! The dome and the axes' spans (PLAN.md M19.1). It adds no numerics, so every vertex is the flash's value at its
//! inputs (`Exact`); a failed cell is left out.

mod common;

use common::{FLUIDS, Tamper, fail, flashed, fluid, is_flash};
use phasekit_core::Pair;
use phasekit_diagram::{Diagram, FluidBatch, Limits, Plane, Quantity, Row, Vertex, diagram, sample};

fn dome_vertices(d: &Diagram) -> Vec<Vertex> {
    let (liquid, vapour) = (d.dome.liquid.iter(), d.dome.vapour.iter());
    liquid.chain(vapour).chain(d.dome.critical.iter()).copied().collect()
}

/// Every vertex of every fluid's dome, on every plane, is the flash's x and y at its inputs, bit for bit.
#[test]
fn dome_vertices_are_the_qt_batch() {
    for name in FLUIDS {
        let f = fluid(name).unwrap();
        for plane in Plane::ALL {
            let d = diagram(f, plane).unwrap();
            assert!(d.dome.liquid.len() > 100 && d.dome.vapour.len() > 100, "{name} {plane:?}");
            for v in dome_vertices(&d) {
                assert!(is_flash(f, plane, &v).unwrap(), "{name} {plane:?} at {:?}", v.inputs);
            }
        }
    }
}

/// Water's dome on T–s runs up the liquid side to just below T_c and down the vapour side, at Q = 0 and 1, from the
/// saturation temperature of the lowest plotted pressure; the critical point is the state at (T_c, ρ_c).
#[test]
fn the_dome_runs_up_the_liquid_side_to_the_critical_point() {
    let water = fluid("Water").unwrap();
    let limits = Limits::of(water).unwrap();
    let d = diagram(water, Plane::Ts).unwrap();
    let t = |v: &Vertex| flashed(water, Quantity::T, v).unwrap();
    assert!(d.dome.liquid.windows(2).all(|w| t(&w[1]) > t(&w[0])));
    assert!(d.dome.liquid.iter().all(|v| v.inputs.pair == Pair::QT && v.inputs.x == 0.0));
    assert!(d.dome.vapour.iter().all(|v| v.inputs.pair == Pair::QT && v.inputs.x == 1.0));
    let critical = d.dome.critical.unwrap().inputs;
    assert_eq!((critical.pair, critical.x, critical.y), (Pair::DT, limits.rho_critical, limits.t_critical));
    let top = t(d.dome.liquid.last().unwrap());
    assert!(1.0 - top / limits.t_critical < 1e-3, "{top}");
    // It enters the plot at the saturation temperature of the lowest plotted pressure, p_c/10⁴ for Water.
    let first = d.dome.liquid.first().unwrap();
    assert!(first.inputs.y > limits.t_triple);
    let p = flashed(water, Quantity::P, first).unwrap();
    assert!((p / (limits.p_critical / 1e4) - 1.0).abs() < 1e-6, "{p}");
}

/// A pseudo-pure fluid's liquid side is its bubble points and its vapour side its dew points: at each temperature the
/// bubble pressure is the higher.
#[test]
fn a_pseudo_pure_fluids_dome_has_its_bubble_and_dew_sides() {
    let r410a = fluid("R410A").unwrap();
    let d = diagram(r410a, Plane::Pt).unwrap();
    let pairs: Vec<_> = d.dome.liquid.iter().zip(&d.dome.vapour).filter(|(l, v)| l.inputs.y == v.inputs.y).collect();
    assert!(pairs.len() > 100);
    assert!(pairs.iter().all(|(bubble, dew)| bubble.y > dew.y), "{:?}", pairs.first());
}

/// A failed cell of the dome is left out of it, on either side, and the rest of the side kept: Water's states between
/// 400 and 410 K failed, every vertex of its whole dome outside them stays.
#[test]
fn a_failed_cell_of_the_dome_is_left_out() {
    let water = fluid("Water").unwrap();
    let limits = Limits::of(water).unwrap();
    let band = |v: &Vertex| 400.0 < v.inputs.y && v.inputs.y < 410.0;
    let broken = Tamper {
        inner: FluidBatch::new(water),
        when: |outputs: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::QT && outputs.len() == 5,
        change: |rows: &mut [Row]| rows.iter_mut().filter(|r| 400.0 < r.inputs.y && r.inputs.y < 410.0).for_each(fail),
    };
    let d = sample(&broken, Plane::Ts, &limits).unwrap();
    let whole = diagram(water, Plane::Ts).unwrap();
    for (side, all) in [(&d.dome.liquid, &whole.dome.liquid), (&d.dome.vapour, &whole.dome.vapour)] {
        assert!(side.iter().all(|v| !band(v) && v.x.is_finite() && v.y.is_finite()));
        assert!(all.iter().any(band) && all.iter().filter(|v| !band(v)).all(|v| side.contains(v)));
    }
}

/// An axis of h, s or ρ spans the dome's and the plotted range's corners' values; an axis of T or p is the range.
#[test]
fn an_axis_of_h_s_or_rho_spans_the_dome_and_the_corners() {
    let water = fluid("Water").unwrap();
    let ts = diagram(water, Plane::Ts).unwrap();
    let xs: Vec<f64> = ts.dome.liquid.iter().chain(&ts.dome.vapour).map(|v| v.x).collect();
    assert!(ts.x.0 <= xs.iter().copied().fold(f64::INFINITY, f64::min));
    assert!(ts.x.1 >= xs.iter().copied().fold(f64::NEG_INFINITY, f64::max));
    assert!(ts.x.1 < 2e4, "{:?}", ts.x);
    let trho = diagram(water, Plane::TRho).unwrap();
    assert!(trho.x.0 > 0.0 && trho.x.1 < 2000.0, "{:?}", trho.x);
    let limits = Limits::of(water).unwrap();
    assert_eq!(trho.y, (limits.t_min.max(limits.t_triple), limits.t_max.min(1.6 * limits.t_critical)));
}
