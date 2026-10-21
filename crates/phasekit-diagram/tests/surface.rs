//! The p–v–T and u–s–v surfaces (PLAN.md M26.1). Every vertex is the batch's state at its inputs (`Exact`); a
//! two-phase vertex of a pure fluid has its temperature's saturation pressure, and lies on the line between its
//! saturated liquid and vapour, within `Flash`; the dome is an edge of the mesh, which no triangle crosses. A
//! pseudo-pure fluid's two-phase `DT` states are its EOS's own VLE, not its bubble and dew curves, so its two-phase
//! vertices are held to its exactness alone.

mod common;

use std::collections::{HashMap, HashSet};

use common::{FLUIDS, Tamper, fail, fluid, plotted_range};
use phasekit_core::{Error, Fluid, Pair, Prop, math};
use phasekit_diagram::{FluidBatch, Inputs, Limits, Quantity, Row, Surface, SurfaceVertex, sample_surface, surface};
use phasekit_verify::tolerance::{ToleranceClass, Window};

/// The five embedded fluids, whose two-phase states are their saturation curves'.
const PURE: [&str; 5] = ["Water", "n-Propane", "CarbonDioxide", "Nitrogen", "R134a"];

/// The state at the inputs, by one flash: T, ρ, p, u, s, Q where it has one, and phase.
fn state_at(f: &Fluid, inputs: Inputs) -> Result<SurfaceVertex, Error> {
    let state = f.state(inputs.input()?)?;
    let get = |prop| f.prop(&state, prop);
    Ok(SurfaceVertex {
        t: get(Prop::T)?,
        rho: get(Prop::Dmass)?,
        p: get(Prop::P)?,
        u: get(Prop::Umass)?,
        s: get(Prop::Smass)?,
        q: get(Prop::Q).ok(),
        phase: state.phase(),
        inputs,
    })
}

/// Whether two vertices are the same state bit for bit.
fn same(a: &SurfaceVertex, b: &SurfaceVertex) -> bool {
    let bits = |v: &SurfaceVertex| [v.t, v.rho, v.p, v.u, v.s].map(f64::to_bits);
    bits(a) == bits(b) && a.q.map(f64::to_bits) == b.q.map(f64::to_bits) && a.phase == b.phase
}

/// The saturated liquid and vapour at T, by the `QT` flash.
fn saturated(f: &Fluid, t: f64) -> Result<[SurfaceVertex; 2], Error> {
    let at = |q: f64| state_at(f, Inputs { pair: Pair::QT, x: q, y: t });
    Ok([at(0.0)?, at(1.0)?])
}

/// Whether `got` is `want` within `Flash` of the magnitude `scale`.
fn within(got: f64, want: f64, scale: f64) -> bool {
    let bound = ToleranceClass::Flash.bound_in(scale, Window::Regular).unwrap_or(0.0);
    (got - want).abs() <= bound
}

/// Every edge of the mesh, each as its two vertices in increasing order.
fn edges(s: &Surface) -> HashSet<(usize, usize)> {
    let ordered = |a: usize, b: usize| (a.min(b), a.max(b));
    let of = |t: &[usize; 3]| [ordered(t[0], t[1]), ordered(t[1], t[2]), ordered(t[2], t[0])];
    s.triangles.iter().flat_map(of).collect()
}

/// Twice the signed area of a triangle in (ln ρ, T): positive where it is counter-clockwise.
fn area(s: &Surface, t: &[usize; 3]) -> f64 {
    let at = |i: usize| (math::ln(s.vertices[i].rho), s.vertices[i].t);
    let [a, b, c] = t.map(at);
    (b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)
}

/// Where a vertex lies against its temperature's dome: below the vapour's density, on the dome, inside or above the
/// liquid's; `None` at a temperature with no dome.
fn side(domes: &HashMap<u64, (f64, f64)>, v: &SurfaceVertex) -> Option<i8> {
    let &(vapour, liquid) = domes.get(&v.t.to_bits())?;
    Some(if v.rho < vapour {
        -2
    } else if v.rho == vapour || v.rho == liquid {
        0
    } else if v.rho < liquid {
        1
    } else {
        2
    })
}

/// Each dome temperature's vapour and liquid densities, and the critical point's density at T_c.
fn domes(s: &Surface) -> HashMap<u64, (f64, f64)> {
    let pairs = s.dome.vapour.iter().zip(&s.dome.liquid);
    let mut all: HashMap<u64, (f64, f64)> =
        pairs.map(|(&v, &l)| (s.vertices[v].t.to_bits(), (s.vertices[v].rho, s.vertices[l].rho))).collect();
    let critical = s.dome.critical.map(|c| &s.vertices[c]);
    all.extend(critical.map(|c| (c.t.to_bits(), (c.rho, c.rho))));
    all
}

/// Whether a triangle has vertices on two sides of the dome: inside and outside it, or on its vapour side and its
/// liquid side. A vertex on the dome is on both.
fn crosses(sides: &[i8]) -> bool {
    let has = |k: i8| sides.contains(&k);
    (has(1) && (has(-2) || has(2))) || (has(-2) && has(2))
}

/// Every vertex of every fluid's surface is the batch's state at its inputs, bit for bit: a `DT` state of the grid or
/// a `QT` state of the dome, with its Q where it is two-phase and the library's phase.
#[test]
fn every_vertex_is_the_dt_state() {
    for name in FLUIDS {
        let f = fluid(name).unwrap();
        let s = surface(f).unwrap();
        assert!(s.vertices.len() > 1000 && s.triangles.len() > 2000, "{name}: {}", s.vertices.len());
        for v in &s.vertices {
            assert!(matches!(v.inputs.pair, Pair::DT | Pair::QT), "{name}: {:?}", v.inputs);
            assert!(same(v, &state_at(f, v.inputs).unwrap()), "{name}: {v:?}");
        }
    }
}

/// On p–v–T the two-phase region is the ruled surface of the saturation pressures: each two-phase vertex's p is its
/// temperature's `QT` pressure within `Flash`.
#[test]
fn the_two_phase_region_is_the_saturation_pressure() {
    for name in PURE {
        let f = fluid(name).unwrap();
        let t_c = Limits::of(f).unwrap().t_critical;
        let s = surface(f).unwrap();
        let two_phase: Vec<_> = s.vertices.iter().filter(|v| v.q.is_some() && v.t < t_c).collect();
        assert!(two_phase.len() > 200, "{name}: {}", two_phase.len());
        for v in two_phase {
            let [liquid, _] = saturated(f, v.t).unwrap();
            assert!(within(v.p, liquid.p, liquid.p), "{name}: {v:?} against {}", liquid.p);
        }
    }
}

/// On u–s–v each two-phase vertex lies on the line between its temperature's saturated liquid and vapour: its u, s and
/// v are its Q's mix of theirs, within `Flash` of the larger.
#[test]
fn a_two_phase_vertex_lies_between_its_saturated_states() {
    for name in PURE {
        let f = fluid(name).unwrap();
        let t_c = Limits::of(f).unwrap().t_critical;
        let s = surface(f).unwrap();
        for v in s.vertices.iter().filter(|v| v.t < t_c) {
            let Some(q) = v.q else { continue };
            assert!((0.0..=1.0).contains(&q), "{name}: {v:?}");
            let [l, g] = saturated(f, v.t).unwrap();
            let mix = |a: f64, b: f64| (1.0 - q) * a + q * b;
            let ok = |got: f64, a: f64, b: f64| within(got, mix(a, b), a.abs().max(b.abs()));
            let usv = ok(v.u, l.u, g.u) && ok(v.s, l.s, g.s) && ok(1.0 / v.rho, 1.0 / l.rho, 1.0 / g.rho);
            assert!(usv, "{name}: {v:?} between {l:?} and {g:?}");
        }
    }
}

/// The saturated liquid and vapour, the critical point and the triple line are edges of the mesh: every dome vertex is
/// a mesh vertex, each side joined from one temperature to the next and its top to the critical point, the triple
/// line along the lowest row. No triangle crosses the dome, every triangle is counter-clockwise in (ln ρ, T), and
/// every vertex is a triangle's. The critical point is a vertex wherever the library gives the state at (T_c, ρ_c):
/// with core's `fast` maths it refuses n-Propane's as `MechanicallyUnstable` (issue #271), and the sides end below it.
#[test]
fn the_dome_is_an_edge_of_the_mesh() {
    let mut through = 0;
    for name in FLUIDS {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let s = surface(f).unwrap();
        holds_the_dome(name, &s);
        assert!(s.dome.liquid.len() > 30, "{name}: {}", s.dome.liquid.len());
        let at = Inputs { pair: Pair::DT, x: limits.rho_critical, y: limits.t_critical };
        let critical = s.dome.critical.map(|c| s.vertices[c]);
        assert_eq!(critical.is_some(), state_at(f, at).is_ok(), "{name}");
        assert!(critical.is_none_or(|c| c.inputs == at), "{name}");
        through += usize::from(critical.is_some());
    }
    assert!(through + 1 >= FLUIDS.len(), "{through}");
}

/// The mesh's checks that hold of any surface, a broken batch's too.
fn holds_the_dome(name: &str, s: &Surface) {
    let e = edges(s);
    let joined = |a: usize, b: usize| e.contains(&(a.min(b), a.max(b)));
    let (liquid, vapour) = (&s.dome.liquid, &s.dome.vapour);
    assert_eq!(liquid.len(), vapour.len(), "{name}");
    for (&l, &g) in liquid.iter().zip(vapour) {
        let (l, g) = (&s.vertices[l], &s.vertices[g]);
        assert_eq!((l.q, g.q, l.t), (Some(0.0), Some(1.0), g.t), "{name}");
    }
    for side in [liquid, vapour] {
        assert!(side.windows(2).all(|w| joined(w[0], w[1])), "{name}");
        let top = side.last().copied();
        assert!(top.zip(s.dome.critical).is_none_or(|(a, b)| joined(a, b)), "{name}");
    }
    assert!(s.dome.triple.windows(2).all(|w| joined(w[0], w[1])), "{name}");
    let d = domes(s);
    for t in &s.triangles {
        assert!(area(s, t) > 0.0, "{name}: {t:?}");
        let sides: Option<Vec<i8>> = t.iter().map(|&i| side(&d, &s.vertices[i])).collect();
        assert!(!sides.is_some_and(|k| crosses(&k)), "{name}: {t:?}");
    }
    let used: HashSet<usize> = s.triangles.iter().flatten().copied().collect();
    assert_eq!(used.len(), s.vertices.len(), "{name}");
}

/// The mesh spans the plotted temperatures, the critical point's row between, and its densities those of the plotted
/// range's corners and the dome; where the range starts at the triple point, its lowest row is the triple line, from
/// the saturated vapour to the saturated liquid.
#[test]
fn the_mesh_spans_the_plotted_range() {
    for name in FLUIDS {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let (t, _) = plotted_range(&limits);
        let s = surface(f).unwrap();
        let ts: Vec<f64> = s.vertices.iter().map(|v| v.t).collect();
        assert_eq!((ts.first(), ts.last()), (Some(&t.0), Some(&t.1)), "{name}");
        assert!(ts.windows(2).all(|w| w[0] <= w[1]), "{name}");
        assert!(ts.contains(&limits.t_critical), "{name}");
        let triple = &s.dome.triple;
        if t.0 == limits.t_triple {
            let ends = (triple.first().copied(), triple.last().copied());
            assert_eq!(ends, (s.dome.vapour.first().copied(), s.dome.liquid.first().copied()), "{name}");
            assert!(triple.iter().all(|&i| s.vertices[i].t == t.0), "{name}");
        } else {
            assert!(triple.is_empty(), "{name}");
        }
    }
}

/// A grid state the batch failed is left out and its row joined across the gap; a temperature whose saturated liquid
/// or vapour failed is left out, row and all, and the dome joined from the row below to the row above.
#[test]
fn a_failed_state_is_left_out_and_the_mesh_joined_across() {
    let water = fluid("Water").unwrap();
    let limits = Limits::of(water).unwrap();
    let batch = FluidBatch::new(water);
    let band = |r: &Row| 400.0 < r.inputs.y && r.inputs.y < 420.0 && r.inputs.x > 10.0;
    let grid = Tamper {
        inner: batch,
        when: |_: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::DT,
        change: |rows: &mut [Row]| rows.iter_mut().filter(|r| band(r)).for_each(fail),
    };
    let s = sample_surface(&grid, &|inputs| batch.phase(inputs), &limits).unwrap();
    holds_the_dome("Water, a band failed", &s);
    let whole = surface(water).unwrap();
    let in_band = |v: &&SurfaceVertex| v.inputs.pair == Pair::DT && 400.0 < v.t && v.t < 420.0 && v.rho > 10.0;
    assert!(whole.vertices.iter().any(|v| in_band(&v)) && !s.vertices.iter().any(|v| in_band(&v)));
    // The dome's row at the first temperature above 450 K loses its vapour.
    let t = whole.dome.vapour.iter().map(|&i| whole.vertices[i].t).find(|&t| t > 450.0).unwrap();
    let dome = Tamper {
        inner: batch,
        when: |_: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::QT,
        change: |rows: &mut [Row]| rows.iter_mut().filter(|r| r.inputs.y == t && r.inputs.x == 1.0).for_each(fail),
    };
    let s = sample_surface(&dome, &|inputs| batch.phase(inputs), &limits).unwrap();
    holds_the_dome("Water, a dome row failed", &s);
    assert!(s.vertices.iter().all(|v| v.t != t));
    assert_eq!(s.dome.liquid.len() + 1, whole.dome.liquid.len());
    // A state whose phase the library refuses is left out too.
    let refused = sample_surface(&batch, &|inputs| batch.phase(inputs).filter(|_| inputs.y != t), &limits).unwrap();
    assert!(refused.vertices.iter().all(|v| v.t != t));
}
