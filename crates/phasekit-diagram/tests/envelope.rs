//! A mixture's envelope (PLAN.md M19.4) on the library's own trace of R410A.mix, from R32's and R125's equations
//! (phasekit-mix, a dev-dependency only): every vertex is the trace's point (`Exact`), drawn bubble side, critical
//! point, dew side; every vertex the mixture's `QT` or `PQ` flash answers at its own Q is that flash's state within
//! `Flash`, and near the critical point, where it refuses both (issue #257), the diagram draws what the trace found;
//! the critical point drawn is the model's, which lies on the envelope within TM15's stated 0.02 K and 2 kPa.

use std::any::Any;
use std::sync::LazyLock;

use phasekit_core::{Basis, Constant, DataSet, Error, Fluid, Input, Pair, Prop, Registry};
use phasekit_diagram::{Inputs, Plane, Trace, Vertex, envelope};
use phasekit_mix::{EnvelopeEnd, EnvelopeOptions, Mixture, Side, with_mixtures};
use phasekit_verify::tolerance::{ToleranceClass, Window};

/// The embedded fluids and CoolProp's mixture names over them.
static MIXTURES: LazyLock<Result<Registry, Error>> =
    LazyLock::new(|| Ok(with_mixtures(Registry::embedded()?, DataSet::Corrected)));

/// The mixture's trace from its bubble point at 1 bar, as the crate takes it: the feed's molar density times its molar
/// mass, Q 0 at a bubble point and 1 at a dew point, and the error that ended it where it did not close.
fn trace(fluid: &Fluid) -> Result<Trace, Error> {
    let not_a_mixture = Error::InvalidInput { quantity: "mixture", value: f64::NAN };
    let mixture: &Mixture = (fluid.model() as &dyn Any).downcast_ref().ok_or(not_a_mixture)?;
    let e = mixture.envelope(&EnvelopeOptions::new())?;
    let end = match e.end {
        EnvelopeEnd::Bound(d) => Some(Error::Domain(d)),
        EnvelopeEnd::NoConvergence(e) => Some(e),
        _ => None,
    };
    let m = fluid.info().molar_mass();
    let rho = e.rho_bulk.iter().map(|rho| rho * m).collect();
    let q = e.side.iter().map(|side| if *side == Side::Bubble { 0.0 } else { 1.0 }).collect();
    Ok(Trace { t: e.t, p: e.p, rho, q, critical: e.critical, end })
}

/// R410A.mix, as its resolver builds it, and its trace.
fn r410a() -> Result<(Fluid, Trace), Error> {
    let fluid = MIXTURES.as_ref().map_err(Clone::clone)?.resolve("R410A.mix")?;
    let trace = trace(&fluid)?;
    Ok((fluid, trace))
}

/// The model's critical point, T_c, p_c and ρ_c.
fn model_critical(fluid: &Fluid) -> Result<[f64; 3], Error> {
    let c = |k| fluid.constant(k);
    Ok([c(Constant::TCritical)?, c(Constant::PCritical)?, c(Constant::RhomassCritical)?])
}

/// On p–T and T–ρ, every vertex is the trace's point bit for bit, the bubble side in the trace's order and the dew side
/// reversed, since the trace starts at the bubble point at 1 bar, each side running toward the critical point; the
/// critical point drawn is the model's, at (T_c, ρ_c).
#[test]
fn every_vertex_is_the_traces_point_drawn_from_the_bubble_side_through_the_critical_point() {
    let (fluid, t) = r410a().unwrap();
    let f = &fluid;
    let [tc, pc, rhoc] = model_critical(f).unwrap();
    for plane in [Plane::Pt, Plane::TRho] {
        let d = envelope(f, &t, plane).unwrap();
        let order: Vec<&Vertex> = d.dome.liquid.iter().chain(d.dome.vapour.iter().rev()).collect();
        assert_eq!(order.len(), t.t.len(), "{plane:?}");
        let xy = |a: f64, p: f64, rho: f64| if plane == Plane::Pt { (a, p) } else { (rho, a) };
        for (k, v) in order.iter().enumerate() {
            let (x, y) = xy(t.t[k], t.p[k], t.rho[k]);
            assert_eq!((v.x.to_bits(), v.y.to_bits()), (x.to_bits(), y.to_bits()), "{plane:?} {k}");
            assert_eq!(v.inputs, Inputs { pair: Pair::QT, x: t.q[k], y: t.t[k] }, "{plane:?} {k}");
        }
        assert!(d.dome.liquid.iter().all(|v| v.inputs.x == 0.0) && d.dome.vapour.iter().all(|v| v.inputs.x == 1.0));
        assert_eq!(d.dome.critical.map(|v| (v.x, v.y)), Some(xy(tc, pc, rhoc)), "{plane:?}");
        assert!(d.lines.is_empty());
    }
}

/// The value of `prop` at the state of `input`, where the flash answers.
fn flashed(f: &Fluid, input: Result<Input, Error>, prop: Prop) -> Option<f64> {
    let state = f.state(input.ok()?).ok()?;
    f.prop(&state, prop).ok()
}

/// Every vertex the mixture's `QT` or `PQ` flash answers at its own Q is that flash's state within `Flash`, 1e-9
/// relative in the p it gives at T or the T it gives at p, and at least two thirds of them are answered: near the
/// critical point the flash refuses both (issue #257), and there the diagram draws what the trace found.
#[test]
fn every_vertex_the_qt_or_pq_flash_answers_is_that_flashs_state_within_flash() {
    let (fluid, t) = r410a().unwrap();
    let f = &fluid;
    let d = envelope(f, &t, Plane::Pt).unwrap();
    let vertices: Vec<&Vertex> = d.dome.liquid.iter().chain(&d.dome.vapour).collect();
    let within = |got: Option<f64>, want: f64| {
        let bound = ToleranceClass::Flash.bound_in(want, Window::Regular).unwrap_or(0.0);
        got.is_some_and(|got| (got - want).abs() <= bound)
    };
    let (mut answered, mut misses) = (0, Vec::new());
    for v in &vertices {
        let (q, temperature, p) = (v.inputs.x, v.x, v.y);
        let at_t = flashed(f, Input::new(Pair::QT, q, temperature, Basis::Mass), Prop::P);
        let at_p = flashed(f, Input::new(Pair::PQ, p, q, Basis::Mass), Prop::T);
        if at_t.is_none() && at_p.is_none() {
            continue;
        }
        answered += 1;
        if !(within(at_t, p) || within(at_p, temperature)) {
            misses.push(format!("Q {q} at {temperature} K, {p} Pa: QT {at_t:?}, PQ {at_p:?}"));
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
    assert!(3 * answered >= 2 * vertices.len(), "{answered} of {}", vertices.len());
}

/// The critical point drawn, the model's, lies on the envelope within TM15's stated 0.02 K and 2 kPa of the trace's
/// crossing, computed at (T_c, ρ_c).
#[test]
fn the_critical_point_drawn_lies_on_the_envelope_within_tm15s_bounds() {
    let (fluid, t) = r410a().unwrap();
    let f = &fluid;
    let [tc, _, rhoc] = model_critical(f).unwrap();
    let (t_cross, p_cross) = t.critical.unwrap();
    let c = envelope(f, &t, Plane::Pt).unwrap().dome.critical.unwrap();
    assert!((c.x - t_cross).abs() <= 0.02 && (c.y - p_cross).abs() <= 2e3, "{c:?} against ({t_cross}, {p_cross})");
    assert_eq!(c.inputs, Inputs { pair: Pair::DT, x: rhoc, y: tc });
}
