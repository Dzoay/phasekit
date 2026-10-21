//! Ice on Water's diagrams (PLAN.md M18.11): the data is each fluid's state at its inputs, and every vertex and corner
//! the data's, bit for bit (`Exact`); the sublimation, melting and saturation lines meet at the triple point, their
//! gaps there measured; inside the solid region, ice's Gibbs energy is the lowest of ice, IAPWS-95's liquid and its
//! vapour.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::density::{self, Branch};
use phasekit_core::{
    Basis, DataSet, DomainPolicy, FlashOptions, Fluid, Input, Order, Pair, Pressure, Quality, Registry, State,
    Temperature,
};
use phasekit_diagram::{
    Corner, ICE_FROM, Ice, Inputs, P_ICE_III, Plane, Quantity, Vertex, diagram, diagram_with_ice, regions,
    regions_with_ice,
};

/// Water under `Corrected` and phasekit-iapws's ice Ih on it.
fn fluids() -> (Fluid, Fluid) {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let water = registry.get("Water").unwrap().clone();
    let ice = phasekit_iapws::ice::ice(&water, DataSet::Corrected).unwrap();
    (water, ice)
}

const PLANES: [Plane; 5] = [Plane::Ph, Plane::Ts, Plane::Pt, Plane::TRho, Plane::Hs];

fn corner(s: &State) -> Corner {
    Corner { t: s.t(), p: s.p(), h: s.h(Basis::Mass), s: s.s(Basis::Mass), rho: s.rho(Basis::Mass) }
}

fn pt(p: f64, t: f64) -> Input {
    Input::pt(Pressure::new(p).unwrap(), Temperature::new(t).unwrap())
}

/// The corner of a PT state, its p and T the inputs.
fn at_pt(fluid: &Fluid, (p, t): (f64, f64), opts: &FlashOptions) -> Corner {
    Corner { p, t, ..corner(&fluid.flash(pt(p, t), opts).unwrap()) }
}

fn extrapolate() -> FlashOptions {
    FlashOptions::new().with_domain(DomainPolicy::Extrapolate)
}

/// The data is each fluid's state at its inputs, bit for bit: the sublimation line's sides ice's QT states at 0 and 1,
/// from 200 K to 273.16 K; the melting line's at Water's melting temperature from 611.657 Pa to 208.566 MPa, the solid
/// ice's PT state and the liquid Water's (extrapolated below 273.16 K); the isobar and the isotherm ice's PT states;
/// a PT state's p and T its inputs.
#[test]
fn the_data_is_each_fluid_s_state() {
    let (water, ice) = fluids();
    let data = Ice::of(&water, &ice, ICE_FROM).unwrap();
    let at_quality =
        |q: f64, t: f64| corner(&ice.state(Input::qt(Quality::new(q).unwrap(), Temperature::new(t).unwrap())).unwrap());
    for [solid, vapour] in &data.sublimation {
        assert_eq!((*solid, *vapour), (at_quality(0.0, solid.t), at_quality(1.0, solid.t)));
    }
    for [solid, liquid] in &data.melting {
        assert_eq!(solid.t, water.model().melting_temperature(solid.p).unwrap());
        assert_eq!(*solid, at_pt(&ice, (solid.p, solid.t), &FlashOptions::new()));
        assert_eq!(*liquid, at_pt(&water, (solid.p, solid.t), &extrapolate()));
    }
    for c in data.top.iter().chain(&data.cold) {
        assert_eq!(*c, at_pt(&ice, (c.p, c.t), &FlashOptions::new()));
    }
    let (first, last) = (data.sublimation[0][0], data.sublimation[data.sublimation.len() - 1][0]);
    assert_eq!((first.t, last.t, last.p), (200.0, 273.16, 611.657));
    let (bottom, top) = (data.melting[0][0], data.melting[data.melting.len() - 1][0]);
    assert_eq!((bottom.p, top.p), (611.657, P_ICE_III));
    assert!(data.top.iter().all(|c| c.p == P_ICE_III) && data.cold.iter().all(|c| c.t == 200.0));
    assert_eq!((data.top[0].t, data.cold[data.cold.len() - 1].p), (top.t, first.p));
}

/// The data's corner as a vertex of `plane`, at its (p, T).
fn on(plane: Plane, c: &Corner) -> Vertex {
    let (x, y) = (c.get(plane.x().quantity).unwrap(), c.get(plane.y().quantity).unwrap());
    Vertex { x, y, inputs: Inputs { pair: Pair::PT, x: c.p, y: c.t } }
}

/// On each plane, ice's sides are the data's corners bit for bit, at their (p, T); Water's dome starts at the triple
/// point; the axes take in the plain diagram's spans and every ice vertex, T's reaching down to 200 K and p's to its
/// sublimation pressure.
#[test]
fn every_ice_vertex_is_the_data_s_on_each_plane() {
    let (water, ice) = fluids();
    let data = Ice::of(&water, &ice, ICE_FROM).unwrap();
    let bottom = data.sublimation[0][0];
    for plane in PLANES {
        let plain = diagram(&water, plane).unwrap();
        let d = diagram_with_ice(&water, plane, &data).unwrap();
        let triple = Inputs { pair: Pair::QT, x: 0.0, y: 273.16 };
        assert_eq!((d.dome.liquid[0].inputs, d.dome.vapour[0].inputs.y), (triple, 273.16), "{plane:?}");
        let lines = d.ice.as_ref().unwrap();
        let side = |line: &[[Corner; 2]], k: usize| line.iter().map(|pair| on(plane, &pair[k])).collect::<Vec<_>>();
        assert_eq!(lines.sublimation.solid, side(&data.sublimation, 0), "{plane:?}");
        assert_eq!(lines.sublimation.fluid, side(&data.sublimation, 1), "{plane:?}");
        assert_eq!(lines.melting.solid, side(&data.melting, 0), "{plane:?}");
        assert_eq!(lines.melting.fluid, side(&data.melting, 1), "{plane:?}");
        let (s, m) = (&lines.sublimation, &lines.melting);
        for v in s.solid.iter().chain(&s.fluid).chain(&m.solid).chain(&m.fluid) {
            assert!((d.x.0..=d.x.1).contains(&v.x) && (d.y.0..=d.y.1).contains(&v.y), "{plane:?}");
        }
        assert!(d.x.0 <= plain.x.0 && plain.x.1 <= d.x.1 && d.y.0 <= plain.y.0 && plain.y.1 <= d.y.1, "{plane:?}");
        for (axis, span) in [(plane.x(), d.x), (plane.y(), d.y)] {
            match axis.quantity {
                Quantity::T => assert_eq!(span.0, 200.0),
                Quantity::P => assert_eq!(span.0, bottom.p),
                _ => {}
            }
        }
    }
}

/// The three lines end at the triple point: the sublimation line at R14-08's (273.16 K, 611.657 Pa), the melting line
/// from Water's curve there too, the saturation line at IAPWS-95's 611.6548 Pa, 2.23e-3 Pa (3.6e-6 of p_t) below. The
/// sides there are Water's saturated phases, so the data and Water's dome share one zero: the melting line's liquid is
/// Water's QT state at 273.16 K within 2.3e-6 J/kg in h (measured); the sublimation line's vapour differs as the gap in
/// p makes it, ρ by 3.6e-6 of itself, s by R ln(p_t/p_sat), 1.7e-3 J/(kg K), h by 2.5e-3 J/kg.
#[test]
fn the_three_lines_meet_at_the_triple_point() {
    let (water, ice) = fluids();
    let data = Ice::of(&water, &ice, ICE_FROM).unwrap();
    let [solid, vapour] = data.sublimation[data.sublimation.len() - 1];
    let [melt_solid, liquid] = data.melting[0];
    assert_eq!([solid.t, solid.p, melt_solid.t, melt_solid.p], [273.16, 611.657, 273.16, 611.657]);
    let dome = diagram_with_ice(&water, Plane::Pt, &data).unwrap().dome;
    let saturated = dome.liquid[0];
    assert_eq!(saturated.x, 273.16);
    let gap = solid.p - saturated.y;
    assert!((gap - 2.228_930e-3).abs() <= 1e-9, "{gap}");
    let at_quality =
        |q: f64| corner(&water.state(Input::qt(Quality::new(q).unwrap(), Temperature::new(273.16).unwrap())).unwrap());
    let off = |a: Corner, b: Corner| [(a.h - b.h).abs(), (a.s - b.s).abs(), ((a.rho - b.rho) / b.rho).abs()];
    let within = |got: [f64; 3], bound: [f64; 3]| got.iter().zip(bound).all(|(g, b)| *g <= b);
    let (liquid_off, vapour_off) = (off(liquid, at_quality(0.0)), off(vapour, at_quality(1.0)));
    assert!(within(liquid_off, [1e-5, 1e-9, 1e-11]), "{liquid_off:?}");
    assert!(within(vapour_off, [3e-3, 2e-3, 4e-6]), "{vapour_off:?}");
}

/// The specific Gibbs energy h − Ts of a state.
fn gibbs(s: &State) -> f64 {
    s.h(Basis::Mass) - s.t() * s.s(Basis::Mass)
}

/// IAPWS-95's specific Gibbs energy at T and molar density ρ, from its own terms: RT(α⁰ + αʳ + 1 + δαʳ_δ)/M.
fn gibbs_95(water: &Fluid, t: f64, rho: f64) -> f64 {
    let eos = water.model().helmholtz().unwrap();
    let (ideal, residual) = (eos.ideal(t, rho, Order::One), eos.residual(t, rho, Order::One));
    let at = |d: &phasekit_core::Derivs, i, j| d.get(i, j).unwrap();
    let g_rt = at(&ideal, 0, 0) + at(&residual, 0, 0) + 1.0 + at(&residual, 0, 1);
    eos.gas_constant() * t * g_rt / water.model().info().molar_mass()
}

/// Inside the solid region, clear of its edges (T from 205 to 270 K, p from ten times the sublimation pressure to
/// 200 MPa, at least 1 K below the melting temperature), ice's Gibbs energy is the lowest of ice, IAPWS-95's liquid and
/// its vapour, each where IAPWS-95 has a mechanically stable root at (T, p), its g from its own terms (core's flash
/// takes no state below 273.16 K but by `Extrapolate`, and a DT state there looks for a dome it has not).
#[test]
fn inside_the_solid_region_ice_has_the_lowest_gibbs_energy() {
    let (water, ice) = fluids();
    let eos = water.model().helmholtz().unwrap();
    let (mut points, mut compared) = (0, 0);
    for t in [205.0, 220.0, 240.0, 255.0, 265.0, 270.0] {
        let p_sub = phasekit_iapws::ice::sublimation_pressure(t).unwrap();
        for k in 0..12 {
            let p = 10.0 * p_sub * phasekit_core::math::powf(200e6 / (10.0 * p_sub), f64::from(k) / 11.0);
            if water.model().melting_temperature(p).is_ok_and(|t_melt| t > t_melt - 1.0) {
                continue;
            }
            points += 1;
            let g_ice = gibbs(&ice.state(pt(p, t)).unwrap());
            for branch in [Branch::Liquid, Branch::Vapour] {
                let Ok(rho) = density::solve(eos, t, p, branch, None) else { continue };
                let g = gibbs_95(&water, t, rho);
                assert!(g_ice < g, "{t} K, {p} Pa, {branch:?}: ice {g_ice}, fluid {g}");
                compared += 1;
            }
        }
    }
    assert_eq!((points, compared), (67, 51));
}

/// Ice's regions follow Water's, whose floor reaches down to ice's lowest pressure: the solid, its outline the two
/// lines' solid sides, the isobar at 208.566 MPa and the isotherm at 200 K; the solid + vapour and the solid + liquid,
/// each its line's solid side and back along its fluid side; every corner the data's.
#[test]
fn ice_s_regions_are_its_lines_and_edges() {
    let (water, ice) = fluids();
    let data = Ice::of(&water, &ice, ICE_FROM).unwrap();
    let plain = regions(&water).unwrap();
    let r = regions_with_ice(&water, &data).unwrap();
    let n = plain.regions.len();
    assert_eq!(r.regions.len(), n + 3);
    let phases = |regions: &[phasekit_diagram::Region]| regions.iter().map(|g| g.phase).collect::<Vec<_>>();
    assert_eq!(phases(&r.regions[..n]), phases(&plain.regions));
    let floor = r.regions[..n].iter().flat_map(|g| &g.outline).map(|c| c.p).fold(f64::INFINITY, f64::min);
    let bottom = data.sublimation[0][0].p;
    assert!((floor - bottom).abs() <= 1e-12 * bottom, "{floor}");
    let phases: Vec<_> = r.regions[n..].iter().map(|g| g.phase).collect();
    assert_eq!(phases, [phasekit_core::Phase::Solid, phasekit_core::Phase::TwoPhase, phasekit_core::Phase::TwoPhase]);
    let side = |line: &[[Corner; 2]], k: usize| line.iter().map(|pair| pair[k]).collect::<Vec<_>>();
    let solid = [side(&data.sublimation, 0), side(&data.melting, 0), data.top.clone(), data.cold.clone()].concat();
    assert_eq!(r.regions[n].outline, solid);
    for (region, line) in r.regions[n + 1..].iter().zip([&data.sublimation, &data.melting]) {
        let back: Vec<Corner> = side(line, 1).into_iter().rev().collect();
        assert_eq!(region.outline, [side(line, 0), back].concat());
    }
}
