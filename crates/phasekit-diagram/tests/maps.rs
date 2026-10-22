//! The property maps (PLAN.md M26.4; user decision DG11). Every vertex of a contour and of a band's polygons is the
//! batch's state at its inputs (`Exact`); a contour vertex is its level within `LEVEL` of the quantity's span, and a
//! band's vertex lies in the band within it; a contour closes, or ends at the plotted range or a gap.

mod common;

use common::{Tamper, fail, flashed, fluid, is_flash};
use phasekit_core::Pair;
use phasekit_diagram::{FluidBatch, LEVEL, Limits, Map, Plane, Quantity, Row, Vertex, map, sample_map};

/// The maps the tests draw: a continuous quantity on p–h, a pseudo-pure fluid's, one with a jump on p–T (h across the
/// saturation line), and a logarithmic quantity on T–ρ. T–s and h–s maps take 1.6 s and 12 s for Water in release,
/// most of it in flashes the batch refuses (`sample_map`'s grid).
const CASES: [(&str, Plane, Quantity); 4] = [
    ("Water", Plane::Ph, Quantity::T),
    ("R410A", Plane::Ph, Quantity::Smass),
    ("CarbonDioxide", Plane::Pt, Quantity::Hmass),
    ("n-Propane", Plane::TRho, Quantity::P),
];

fn tolerance(m: &Map) -> f64 {
    LEVEL * (m.values.1 - m.values.0)
}

/// Each contour's vertex is the state at its inputs, at its level within `LEVEL` of the quantity's span over the grid;
/// the levels lie strictly inside that span, in order; each band's polygon vertex is a state at its inputs whose
/// quantity lies in the band within the same.
#[test]
fn each_contour_vertex_is_its_level() {
    for (name, plane, q) in CASES {
        let f = fluid(name).unwrap();
        let m = map(f, plane, q).unwrap();
        let (tol, levels) = (tolerance(&m), m.contours.iter().map(|c| c.level).collect::<Vec<_>>());
        assert!(levels.len() >= 5 && levels.windows(2).all(|w| w[0] < w[1]), "{name} {plane:?}: {levels:?}");
        assert!(m.values.0 < levels[0] && levels[levels.len() - 1] < m.values.1, "{name} {plane:?}");
        assert!(m.contours.iter().filter(|c| !c.lines.is_empty()).count() + 1 >= levels.len(), "{name} {plane:?}");
        let value = |v: &Vertex| flashed(f, q, v).unwrap();
        for c in &m.contours {
            for v in c.lines.iter().flatten() {
                assert!(is_flash(f, plane, v).unwrap(), "{name} {plane:?} {v:?}");
                assert!((value(v) - c.level).abs() <= tol, "{name} {plane:?} {}: {v:?}", c.level);
            }
        }
        let bounds: Vec<_> = m.bands.iter().map(|b| (b.low, b.high)).collect();
        let below = [None].into_iter().chain(levels.iter().copied().map(Some));
        let above = levels.iter().copied().map(Some).chain([None]);
        assert_eq!(bounds, below.zip(above).collect::<Vec<_>>(), "{name} {plane:?}");
        let inside = |v: f64, low: Option<f64>, high: Option<f64>| {
            low.is_none_or(|l| v >= l - tol) && high.is_none_or(|h| v <= h + tol)
        };
        for b in &m.bands {
            for v in b.polygons.iter().flatten() {
                assert!(is_flash(f, plane, v).unwrap() && inside(value(v), b.low, b.high), "{name} {plane:?} {v:?}");
            }
        }
        let polygons: usize = m.bands.iter().map(|b| b.polygons.len()).sum();
        assert!(polygons > 3000 && m.bands.iter().flat_map(|b| &b.polygons).all(|p| p.len() >= 3), "{name} {plane:?}");
    }
}

/// Whether the vertex's inputs (ρ, T) lie on the T–ρ map's range.
fn on_range(m: &Map, v: &Vertex) -> bool {
    [m.x.0, m.x.1].contains(&v.inputs.x) || [m.y.0, m.y.1].contains(&v.inputs.y)
}

/// n-Propane's pressure is continuous over the T–ρ plane, and the batch fails nowhere there: each contour line ends on
/// the plotted range. Failing the states from 300 K to 320 K splits each line that crossed them: none has a vertex
/// there, and each end not on the range lies within a grid step (8 K) of them.
#[test]
fn a_contour_closes_or_ends_at_the_range_or_a_gap() {
    let f = fluid("n-Propane").unwrap();
    let whole = map(f, Plane::TRho, Quantity::P).unwrap();
    let ends = |l: &Vec<Vertex>| [l[0], l[l.len() - 1]];
    for line in whole.contours.iter().flat_map(|c| &c.lines) {
        let closed = line[0].inputs == line[line.len() - 1].inputs;
        assert!(closed || ends(line).iter().all(|v| on_range(&whole, v)), "{line:?}");
    }
    let band = |r: &Row| 300.0 < r.inputs.y && r.inputs.y < 320.0;
    let tamper = Tamper {
        inner: FluidBatch::new(f),
        when: |_: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::DT,
        change: |rows: &mut [Row]| rows.iter_mut().filter(|r| band(r)).for_each(fail),
    };
    let cut = sample_map(&tamper, Plane::TRho, Quantity::P, &Limits::of(f).unwrap()).unwrap();
    let count = |m: &Map| m.contours.iter().map(|c| c.lines.len()).sum::<usize>();
    assert!(count(&cut) > count(&whole));
    for line in cut.contours.iter().flat_map(|c| &c.lines) {
        assert!(line.iter().all(|v| !(300.0 < v.inputs.y && v.inputs.y < 320.0)), "{line:?}");
        let near_gap = |v: &Vertex| (292.0..=328.0).contains(&v.inputs.y);
        assert!(ends(line).iter().all(|v| on_range(&cut, v) || near_gap(v)), "{line:?}");
    }
}
