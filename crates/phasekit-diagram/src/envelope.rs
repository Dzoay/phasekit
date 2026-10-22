//! A mixture's diagram (PLAN.md M19.4, the site's `envelope.ts` of M21.5): its phase envelope on p–T and T–ρ as its
//! trace gave it, each vertex a bubble or a dew point of the feed, and the critical point the trace passed, drawn where
//! the model puts it (`Fluid::constant`, which `Props1SI` reads). The trace reaches the crate as data, so that it
//! depends on core alone (user decision DG3). It adds no numerics: every vertex is the trace's.

use phasekit_core::{Constant, Error, Fluid, Pair};

use crate::batch::{Inputs, Vertex};
use crate::diagram::{Diagram, Dome};
use crate::plane::Plane;

/// A mixture's phase envelope as its trace gave it (phasekit-mix's `Envelope`, PLAN.md M13.12): one entry a point, in
/// the trace's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    /// Temperature, K.
    pub t: Vec<f64>,
    /// Pressure, Pa.
    pub p: Vec<f64>,
    /// The feed's mass density, kg/m³.
    pub rho: Vec<f64>,
    /// The feed's vapour quality: 0 at a bubble point, where the feed is the denser phase, 1 at a dew point.
    pub q: Vec<f64>,
    /// The critical point the trace passed, (T, p); `None` where it passed none.
    pub critical: Option<(f64, f64)>,
    /// The error that ended the trace where it did not close; a trace without a point is refused with it.
    pub end: Option<Error>,
}

/// The mixture's diagram on `plane` from its traced envelope: on p–T and T–ρ, the bubble side as the dome's liquid and
/// the dew side as its vapour, each running toward the critical point as a pure fluid's does, and the model's critical
/// point where the trace passed one and the model gives T_c, p_c and ρ_c. Each axis spans the vertices, a logarithmic
/// one widened by a factor of 2 each way, a linear one by a tenth of its span. The other planes are `Unsupported`
/// (their pair); a trace without a point is refused with the error that ended it, and one whose columns differ in
/// length is a `Shape` error.
pub fn envelope(fluid: &Fluid, trace: &Trace, plane: Plane) -> Result<Diagram, Error> {
    let critical = trace.critical.and_then(|_| critical_of(|k| fluid.constant(k)));
    drawn(trace, plane, critical)
}

/// The model's critical point, T_c, p_c and ρ_c, where `constant` gives each finite.
fn critical_of(constant: impl Fn(Constant) -> Result<f64, Error>) -> Option<[f64; 3]> {
    let c = |k| constant(k).ok().filter(|v: &f64| v.is_finite());
    Some([c(Constant::TCritical)?, c(Constant::PCritical)?, c(Constant::RhomassCritical)?])
}

/// The envelope on `plane`, with the model's critical point `critical`, T_c, p_c and ρ_c, where it is drawn.
fn drawn(trace: &Trace, plane: Plane, critical: Option<[f64; 3]>) -> Result<Diagram, Error> {
    if !matches!(plane, Plane::Pt | Plane::TRho) {
        let (x, y) = (plane.x().quantity.var(), plane.y().quantity.var());
        return Err(Error::Unsupported { pair: Pair::from_vars(x, y).map_or(Pair::PT, |(pair, _)| pair) });
    }
    let n = trace.t.len();
    if let Some(found) = [trace.p.len(), trace.rho.len(), trace.q.len()].into_iter().find(|&len| len != n) {
        return Err(Error::Shape { expected: n, found });
    }
    if n == 0 {
        return Err(trace
            .end
            .clone()
            .unwrap_or(Error::InvalidArgument { argument: "envelope points", value: Some(0) }));
    }
    let at = |[t, p, rho]: [f64; 3]| if plane == Plane::Pt { (t, p) } else { (rho, t) };
    let points = trace.t.iter().zip(&trace.p).zip(&trace.rho).zip(&trace.q);
    let vertices: Vec<(f64, Vertex)> = points
        .map(|(((&t, &p), &rho), &q)| {
            let (x, y) = at([t, p, rho]);
            (q, Vertex { x, y, inputs: Inputs { pair: Pair::QT, x: q, y: t } })
        })
        .collect();
    let critical = critical.map(|[t, p, rho]| {
        let (x, y) = at([t, p, rho]);
        Vertex { x, y, inputs: Inputs { pair: Pair::DT, x: rho, y: t } }
    });
    let dome = Dome { liquid: side(&vertices, 0.0), vapour: side(&vertices, 1.0), critical };
    let all: Vec<&Vertex> = dome.liquid.iter().chain(&dome.vapour).chain(&dome.critical).collect();
    let x = around(extent(all.iter().map(|v| v.x)), plane.x().log);
    let y = around(extent(all.iter().map(|v| v.y)), plane.y().log);
    Ok(Diagram { plane, dome, lines: Vec::new(), x, y })
}

/// The points of the side at quality `q`: in the trace's order where the trace starts on it, which then runs toward
/// the critical point, else reversed, so that it runs toward it too.
fn side(points: &[(f64, Vertex)], q: f64) -> Vec<Vertex> {
    let mut out: Vec<Vertex> = points.iter().filter(|(at, _)| *at == q).map(|(_, v)| *v).collect();
    if points.first().is_some_and(|(at, _)| *at != q) {
        out.reverse();
    }
    out
}

/// The least and greatest of the values.
fn extent(values: impl Iterator<Item = f64>) -> (f64, f64) {
    values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

/// A span around [lo, hi]: a logarithmic axis's widened by a factor of 2 each way, a linear axis's by a tenth of its
/// span, or of its value where the span is 0.
fn around((lo, hi): (f64, f64), log: bool) -> (f64, f64) {
    if log {
        return (lo / 2.0, hi * 2.0);
    }
    let span = hi - lo;
    let pad = if span == 0.0 { hi.abs() } else { span } / 10.0;
    (lo - pad, hi + pad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use phasekit_core::Strategy;

    /// A trace of points (Q, T, p, ρ) in its order, that passed a critical point or none.
    fn traced(points: &[[f64; 4]], critical: Option<(f64, f64)>) -> Trace {
        let column = |i: usize| points.iter().map(|p| p.get(i).copied().unwrap_or(f64::NAN)).collect();
        Trace { q: column(0), t: column(1), p: column(2), rho: column(3), critical, end: None }
    }

    const CRITICAL: [f64; 3] = [301.0, 5.1e6, 400.0];

    fn xy(vertices: &[Vertex]) -> Vec<(f64, f64)> {
        vertices.iter().map(|v| (v.x, v.y)).collect()
    }

    /// The site's (M21.5): from the dew side where the trace starts there, each side toward the critical point, the
    /// model's drawn at (T_c, ρ_c).
    #[test]
    fn each_side_runs_toward_the_critical_point_from_the_dew_side_where_the_trace_starts_there() {
        let points =
            [[1.0, 200.0, 1e5, 2.0], [1.0, 280.0, 3e6, 100.0], [0.0, 290.0, 4e6, 600.0], [0.0, 210.0, 1e5, 1200.0]];
        let d = drawn(&traced(&points, Some((300.0, 5e6))), Plane::Pt, Some(CRITICAL)).unwrap();
        assert_eq!(xy(&d.dome.liquid), [(210.0, 1e5), (290.0, 4e6)]);
        assert_eq!(xy(&d.dome.vapour), [(200.0, 1e5), (280.0, 3e6)]);
        let critical = Vertex { x: 301.0, y: 5.1e6, inputs: Inputs { pair: Pair::DT, x: 400.0, y: 301.0 } };
        assert_eq!(d.dome.critical, Some(critical));
        assert_eq!(d.dome.liquid[0].inputs, Inputs { pair: Pair::QT, x: 0.0, y: 210.0 });
        assert!(d.lines.is_empty());
    }

    /// On T–ρ, the feed's density against its temperature, each axis widened: ρ's, logarithmic, by 2 each way.
    #[test]
    fn on_t_rho_the_feeds_density_against_its_temperature() {
        let points = [[0.0, 210.0, 1e5, 1200.0], [1.0, 200.0, 1e5, 2.0]];
        let d = drawn(&traced(&points, Some((300.0, 5e6))), Plane::TRho, Some(CRITICAL)).unwrap();
        assert_eq!([d.dome.liquid[0].x, d.dome.liquid[0].y, d.dome.vapour[0].x], [1200.0, 210.0, 2.0]);
        assert_eq!(d.dome.critical.map(|v| (v.x, v.y)), Some((400.0, 301.0)));
        assert_eq!((d.x, d.y), ((1.0, 2400.0), (189.9, 311.1)));
    }

    /// No critical point where the trace passed none, or the model lacks one of T_c, p_c and ρ_c or gives it not
    /// finite.
    #[test]
    fn no_critical_point_where_the_trace_passed_none_or_the_model_gives_none() {
        let open = traced(&[[0.0, 216.0, 7e6, 1100.0]], None);
        let d = drawn(&open, Plane::Pt, None).unwrap();
        assert_eq!((d.dome.liquid.len(), d.dome.vapour.len(), d.dome.critical), (1, 0, None));
        assert_eq!((d.x, d.y), ((194.4, 237.6), (3.5e6, 1.4e7)));
        let missing = Error::NoModel { prop: phasekit_core::Prop::T };
        for absent in [Constant::TCritical, Constant::PCritical, Constant::RhomassCritical] {
            let lacks = |k| if k == absent { Err(missing.clone()) } else { Ok(1.0) };
            let nan = |k| Ok(if k == absent { f64::NAN } else { 1.0 });
            assert_eq!((critical_of(lacks), critical_of(nan)), (None, None), "{absent:?}");
        }
        let model = |k| {
            Ok(match k {
                Constant::TCritical => 301.0,
                Constant::PCritical => 5.1e6,
                _ => 400.0,
            })
        };
        assert_eq!(critical_of(model), Some(CRITICAL));
    }

    /// A trace without a point is refused with the error that ended it; columns of different lengths are a `Shape`
    /// error; the other planes are `Unsupported`, their pair named.
    #[test]
    fn a_trace_without_a_point_is_refused_in_its_own_words() {
        let failed = Error::NoConvergence { strategy: Strategy::Vle, iterations: 0 };
        let empty = Trace { end: Some(failed.clone()), ..Trace::default() };
        assert_eq!(drawn(&empty, Plane::Pt, Some(CRITICAL)), Err(failed));
        assert!(matches!(drawn(&Trace::default(), Plane::Pt, None), Err(Error::InvalidArgument { .. })));
        let short = Trace { rho: Vec::new(), ..traced(&[[0.0, 216.0, 7e6, 1100.0]], None) };
        assert_eq!(drawn(&short, Plane::TRho, None), Err(Error::Shape { expected: 1, found: 0 }));
        let one = traced(&[[0.0, 216.0, 7e6, 1100.0]], None);
        let pairs = [(Plane::Ph, Pair::HP), (Plane::Ts, Pair::ST), (Plane::Hs, Pair::HS)];
        for (plane, pair) in pairs {
            assert_eq!(drawn(&one, plane, None), Err(Error::Unsupported { pair }), "{plane:?}");
        }
    }

    /// The site's ranges (M21.5): a logarithmic axis widened by 2 each way, a linear one by a tenth of its span, or of
    /// its value where the span is 0.
    #[test]
    fn a_logarithmic_axis_widens_by_2_and_a_linear_one_by_a_tenth() {
        assert_eq!(around((1e5, 1e6), true), (5e4, 2e6));
        assert_eq!(around((200.0, 300.0), false), (190.0, 310.0));
        assert_eq!(around((200.0, 200.0), false), (180.0, 220.0));
        assert_eq!(around((-200.0, -200.0), false), (-220.0, -180.0));
    }
}
