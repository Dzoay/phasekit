//! The characteristic curves and the compressibility chart (PLAN.md M26.3; user decision DG10). Every vertex is the
//! batch's state at its inputs (`Exact`); a curve's vertex is a root of its condition, by the library's own derivative
//! at the vertex's state; a curve meets zero density where the second virial coefficient says, where that lies below
//! the equation of state's highest temperature; and a chart's Z is p/(ρRT).

mod common;

use common::{FLUIDS, Tamper, fail, fluid, plotted_range};
use phasekit_core::{Constant, Error, Fluid, Pair, Prop, math};
use phasekit_diagram::{
    Characteristic, Curve, CurveVertex, FluidBatch, Inputs, Limits, Quantity, Row, curves, sample_curves,
};

/// How near zero a curve's condition is at its vertices: bisection leaves a bracket of 10⁻¹³ of its input, and the
/// conditions reach 1.1e-12 there (Air's ideal curve).
const ROOT: f64 = 1e-10;

/// The value of each quantity at the state of the inputs, by one flash.
fn flashed<const N: usize>(f: &Fluid, inputs: Inputs, quantities: [Quantity; N]) -> Result<[f64; N], Error> {
    let state = f.state(inputs.input()?)?;
    let mut out = [0.0; N];
    for (o, q) in out.iter_mut().zip(quantities) {
        *o = f.prop(&state, q.prop())?;
    }
    Ok(out)
}

/// Whether the vertex's T, ρ, p and Z are, bit for bit, the flash's at its inputs.
fn is_flash(f: &Fluid, v: &CurveVertex) -> Result<bool, Error> {
    let want = flashed(f, v.inputs, [Quantity::T, Quantity::Dmass, Quantity::P, Quantity::Z])?;
    Ok(want.map(f64::to_bits) == [v.t, v.rho, v.p, v.z].map(f64::to_bits))
}

/// The curve's condition at the vertex's state, by the library's derivative there: Z − 1, (ρ/p)(∂p/∂ρ)_T − 1,
/// (p/T)(∂T/∂p)_h and (T/p)(∂p/∂T)_ρ − 1.
fn condition(f: &Fluid, c: Characteristic, v: &CurveVertex) -> Result<f64, Error> {
    let derivative = match c {
        Characteristic::Ideal => Quantity::Z,
        Characteristic::Boyle => Quantity::DpDrhoAtT,
        Characteristic::JouleThomsonInversion => Quantity::JouleThomson,
        Characteristic::JouleInversion => Quantity::DpDTAtRho,
    };
    let [p, t, rho, z, d] = flashed(f, v.inputs, [Quantity::P, Quantity::T, Quantity::Dmass, Quantity::Z, derivative])?;
    Ok(match c {
        Characteristic::Ideal => z - 1.0,
        Characteristic::Boyle => rho * d / p - 1.0,
        Characteristic::JouleThomsonInversion => p * d / t,
        Characteristic::JouleInversion => t * d / p - 1.0,
    })
}

/// The curve's virial condition at T, by the library's B and dB/dT at a dilute state: B, T dB/dT − B or dB/dT.
fn virial(f: &Fluid, c: Characteristic, t: f64) -> Result<f64, Error> {
    let dilute = Inputs { pair: Pair::DT, x: 1e-6, y: t };
    let [b, db] = flashed(f, dilute, [Quantity::Bvirial, Quantity::DBvirialDT])?;
    Ok(match c {
        Characteristic::Ideal | Characteristic::Boyle => b,
        Characteristic::JouleThomsonInversion => t * db - b,
        Characteristic::JouleInversion => db,
    })
}

fn curve(all: &[Curve], c: Characteristic) -> Option<&Curve> {
    all.iter().find(|curve| curve.characteristic == c)
}

/// The fluids whose ranges the Joule inversion curve crosses, at liquid-like densities toward T_max.
const JOULE: [&str; 4] = ["n-Propane", "CarbonDioxide", "Nitrogen", "Air"];

/// Each curve's vertex is the `DT` state at its inputs and a root of the curve's condition, within `ROOT`; the ideal,
/// Boyle and Joule–Thomson inversion curves cross every fluid's range, and the Joule inversion curve `JOULE`'s.
#[test]
fn each_vertex_meets_its_curves_condition() {
    for name in FLUIDS {
        let f = fluid(name).unwrap();
        let all = curves(f).unwrap().curves;
        let order: Vec<Characteristic> = all.iter().map(|c| c.characteristic).collect();
        assert_eq!(order, Characteristic::ALL, "{name}");
        for c in &all {
            let crossing = c.characteristic != Characteristic::JouleInversion || JOULE.contains(&name);
            assert!(!crossing || c.lines.iter().any(|l| l.len() > 10), "{name} {:?}", c.characteristic);
            for v in c.lines.iter().flatten() {
                assert!(v.inputs.pair == Pair::DT && is_flash(f, v).unwrap(), "{name} {:?} {v:?}", c.characteristic);
                let got = condition(f, c.characteristic, v).unwrap();
                assert!(got.abs() <= ROOT, "{name} {:?} at {v:?}: {got:e}", c.characteristic);
            }
            assert!(c.lines.iter().all(|l| l.len() > 1 && l[0].rho <= l[l.len() - 1].rho), "{name}");
        }
    }
}

/// Where each curve meets zero density, by CoolProp 8.0.0's virials (DG10): the ideal and Boyle curves where B = 0
/// below Water's, CarbonDioxide's, Nitrogen's and Air's highest temperatures, the Joule–Thomson inversion curve where
/// T dB/dT = B below CarbonDioxide's, Nitrogen's and Air's, and the Joule inversion curve, where dB/dT = 0, below none.
const MEETS: [(Characteristic, &[&str]); 4] = [
    (Characteristic::Ideal, &["Water", "CarbonDioxide", "Nitrogen", "Air"]),
    (Characteristic::Boyle, &["Water", "CarbonDioxide", "Nitrogen", "Air"]),
    (Characteristic::JouleThomsonInversion, &["CarbonDioxide", "Nitrogen", "Air"]),
    (Characteristic::JouleInversion, &[]),
];

/// A curve's zero-density end is a root of its virial condition, within 10⁻¹⁰ of its temperature, and the curve's
/// lowest-density vertex lies within ρ/ρ_c of it (relatively): the virial series' first-order term, whose coefficient
/// the fluids hold below 0.6. Where a curve has no end, its condition keeps one sign from the plotted range's lowest
/// temperature to the equation of state's highest.
#[test]
fn the_curves_zero_density_ends_are_the_virials() {
    for name in FLUIDS {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let all = curves(f).unwrap().curves;
        let (low, high) = (plotted_range(&limits).0.0, limits.t_max);
        for (c, fluids) in MEETS {
            let curve = curve(&all, c).unwrap();
            let sign = |t: f64| virial(f, c, t).unwrap() >= 0.0;
            match curve.zero_density {
                Some(t) => {
                    assert!(fluids.contains(&name), "{name} {c:?} at {t}");
                    assert_ne!(sign(t * (1.0 - 1e-10)), sign(t * (1.0 + 1e-10)), "{name} {c:?} at {t}");
                    let near = curve.lines.iter().flatten().min_by(|a, b| a.rho.total_cmp(&b.rho)).unwrap();
                    let (delta, gap) = (near.rho / limits.rho_critical, (near.t / t - 1.0).abs());
                    assert!(delta < 0.01 && gap <= delta, "{name} {c:?}: {near:?} against {t}");
                }
                None => {
                    assert!(!fluids.contains(&name), "{name} {c:?}");
                    let ts = (0..=200).map(|i| low * math::powf(high / low, f64::from(i) / 200.0));
                    let signs: Vec<bool> = ts.map(sign).collect();
                    assert!(signs.iter().all(|&s| s == signs[0]), "{name} {c:?}");
                }
            }
        }
    }
}

/// Each isotherm's vertex is the state at its inputs, its Z p/(ρRT) within 10⁻¹³; it runs in order of pressure at its
/// own temperature, and below T_c it has the saturated vapour and then the saturated liquid at the saturation pressure.
#[test]
fn z_is_p_over_rho_r_t() {
    for name in FLUIDS {
        let f = fluid(name).unwrap();
        let limits = Limits::of(f).unwrap();
        let r = f.constant(Constant::GasConstant).unwrap() / f.constant(Constant::MolarMass).unwrap();
        let chart = curves(f).unwrap().compressibility;
        assert!(chart.len() >= 5, "{name}");
        assert!(chart.windows(2).all(|w| w[0].t < w[1].t), "{name}");
        for iso in &chart {
            let vs = &iso.vertices;
            assert!(vs.len() > 20, "{name} {}", iso.t);
            for v in vs {
                assert!(is_flash(f, v).unwrap() && v.t == iso.t, "{name} {v:?}");
                assert!((v.z / (v.p / (v.rho * r * v.t)) - 1.0).abs() <= 1e-13, "{name} {v:?}");
            }
            assert!(vs.windows(2).all(|w| w[0].p <= w[1].p), "{name} {}", iso.t);
            let saturated: Vec<(usize, f64)> = vs
                .iter()
                .enumerate()
                .filter(|(_, v)| v.inputs.pair == Pair::QT)
                .map(|(i, v)| (i, v.inputs.x))
                .collect();
            if iso.t < limits.t_critical {
                let (vapour, liquid) = (saturated[0], saturated[1]);
                assert_eq!(
                    (saturated.len(), vapour.1, liquid.1, liquid.0),
                    (2, 1.0, 0.0, vapour.0 + 1),
                    "{name} {}",
                    iso.t
                );
                let q = |v: &CurveVertex| flashed(f, v.inputs, [Quantity::Q]).ok().map(|[q]| q);
                assert!(vs.iter().filter(|v| v.inputs.pair == Pair::PT).all(|v| q(v).is_none()), "{name} {}", iso.t);
            } else {
                assert!(saturated.is_empty(), "{name} {}", iso.t);
            }
        }
    }
}

/// A line ends where the batch fails: failing Water's states from 900 K to 1000 K splits its ideal curve in two, with
/// no vertex between.
#[test]
fn a_line_ends_where_the_batch_fails() {
    let water = fluid("Water").unwrap();
    let limits = Limits::of(water).unwrap();
    let whole = curves(water).unwrap().curves;
    assert_eq!(curve(&whole, Characteristic::Ideal).unwrap().lines.len(), 1);
    let band = |r: &Row| 900.0 < r.inputs.y && r.inputs.y < 1000.0;
    let tamper = Tamper {
        inner: FluidBatch::new(water),
        when: |_: &[Quantity], pair: Pair, _: &[f64], _: &[f64]| pair == Pair::DT,
        change: |rows: &mut [Row]| rows.iter_mut().filter(|r| band(r)).for_each(fail),
    };
    let cut = sample_curves(&tamper, &limits).unwrap().curves;
    assert_eq!(curve(&cut, Characteristic::Ideal).unwrap().lines.len(), 2);
    let between = |v: &CurveVertex| 900.0 < v.t && v.t < 1000.0;
    assert!(cut.iter().flat_map(|c| c.lines.iter().flatten()).all(|v| !between(v)));
    assert!(whole.iter().flat_map(|c| c.lines.iter().flatten()).any(between));
    // The two-phase states are no curve's: Prop::Q is a value at none of the vertices.
    let q = |v: &CurveVertex| water.state(v.inputs.input().unwrap()).and_then(|s| water.prop(&s, Prop::Q)).ok();
    assert!(whole.iter().flat_map(|c| c.lines.iter().flatten()).all(|v| q(v).is_none()));
}
