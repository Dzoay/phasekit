//! Batch on real fluids (PLAN.md M5.8; VERIFICATION.md §9.1; ARCHITECTURE.md §7): the batch driver is the scalar path
//! cell by cell, bit for bit, into point-major buffers.

#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::batch::{self, BatchRequest, Status};
use phasekit_core::{Basis, DataSet, Density, Input, Pair, Prop, Registry, Temperature};
use phasekit_verify::SplitMix64;

/// Outputs that cover single-phase and two-phase answers and refusals (cp, w and Q are undefined somewhere).
const OUTPUTS: [Prop; 8] =
    [Prop::T, Prop::P, Prop::Dmolar, Prop::Hmolar, Prop::Smolar, Prop::Cpmolar, Prop::SpeedOfSound, Prop::Q];

/// `n` DT points of a fluid between its triple point and 1.5 T_c, ρ log-spaced over six decades below its saturated
/// liquid at the triple point: gas, liquid, supercritical and two-phase states.
fn points(registry: &Registry, name: &str, n: usize, seed: u64) -> (Vec<f64>, Vec<f64>) {
    let record = phasekit_core::internal::record(registry, name).unwrap();
    let curve = record.superancillary_curve().unwrap();
    let (t_lo, t_c) = curve.t_range();
    let rho_l = curve.at_t(t_lo).unwrap().bubble.rho;
    let mut rng = SplitMix64::new(seed);
    let mut sample =
        |_| (rng.log_uniform(1e-6 * rho_l, rho_l), rng.uniform(t_lo, (1.5 * t_c).min(record.limits.t_max())));
    (0..n).map(&mut sample).unzip()
}

/// VERIFICATION.md §9.1, class `Exact`: 10,000 DT points of each of Water, CarbonDioxide and R1234yf through
/// `batch::evaluate`, every cell bitwise the scalar flash and `Fluid::prop` of that point, and every status the scalar
/// result's (ROT-011, ROT-014: one code path, so batch and scalar labels and values never disagree).
#[test]
fn batch_equals_scalar_bitwise() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    for (seed, name) in [(1, "Water"), (2, "CarbonDioxide"), (3, "R1234yf")] {
        let fluid = registry.get(name).unwrap();
        let (x, y) = points(&registry, name, 10_000, seed);
        let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &OUTPUTS);
        let (mut out, mut status) = (vec![0.0; x.len() * OUTPUTS.len()], vec![Status::Other; x.len() * OUTPUTS.len()]);
        batch::evaluate(fluid, &req, &mut out, &mut status).unwrap();
        let (mut two_phase, mut refused) = (0, 0);
        for (i, (rho, t)) in x.iter().zip(&y).enumerate() {
            let state = fluid.state(Input::dt(Density::molar(*rho).unwrap(), Temperature::new(*t).unwrap()));
            two_phase += usize::from(state.as_ref().is_ok_and(|s| s.quality().is_some()));
            for (k, prop) in OUTPUTS.iter().enumerate() {
                let scalar = state.as_ref().map_err(Clone::clone).and_then(|s| fluid.prop(s, *prop));
                let (cell, cell_status) = (out[i * OUTPUTS.len() + k], status[i * OUTPUTS.len() + k]);
                match scalar {
                    Ok(v) => assert_eq!(
                        (cell.to_bits(), cell_status),
                        (v.to_bits(), Status::Ok),
                        "{name} point {i} {prop:?}"
                    ),
                    Err(e) => {
                        assert!(cell.is_nan() && cell_status == Status::from(&e), "{name} point {i} {prop:?}: {e:?}");
                        refused += 1;
                    }
                }
            }
        }
        assert!(two_phase > 100 && refused > 0, "{name}: {two_phase} two-phase points, {refused} refused cells");
    }
}

/// User decision 11 (ARCHITECTURE.md §7): `out[i·M + k]` is output k of point i, and `status` has the same layout.
#[test]
fn out_is_point_major() {
    let registry = Registry::from_embedded(DataSet::Corrected).unwrap();
    let water = registry.get("Water").unwrap();
    let (x, y) = ([10.0, 55_400.0, 1_000.0], [700.0, 300.0, 400.0]);
    let outputs = [Prop::T, Prop::Dmolar, Prop::Q];
    let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &outputs);
    let (mut out, mut status) = ([0.0; 9], [Status::Other; 9]);
    batch::evaluate(water, &req, &mut out, &mut status).unwrap();
    assert_eq!([out[0], out[3], out[6]], y, "column 0 is T");
    for (i, rho) in x.iter().enumerate() {
        assert!((out[i * 3 + 1] / rho - 1.0).abs() < 1e-15, "column 1 is ρ: {}", out[i * 3 + 1]);
    }
    assert_eq!([status[2], status[5]], [Status::Undefined, Status::Undefined], "single phases have no Q");
    assert!(status[8] == Status::Ok && out[8] > 0.0 && out[8] < 1.0, "the third point is two-phase: {}", out[8]);
}
