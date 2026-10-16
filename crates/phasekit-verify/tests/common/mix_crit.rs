//! The oracle's mixture critical points (`coolprop-8.0.0/all/mix_crit.csv`, PLAN.md M13.7): its rows, the binaries
//! they belong to, and the offsets of phasekit's nearest point.

#![allow(dead_code, reason = "each test binary that includes the rows uses its part of them")]

use phasekit_core::{DataSet, Fluid, Registry, math};
use phasekit_mix::{CriticalState, Mixture};
use phasekit_verify::{Fixture, fixture};

/// One of `all_critical_points`' points.
pub struct OraclePoint {
    /// The binary, "A&B".
    pub label: String,
    /// The first component's mole fraction.
    pub z1: f64,
    /// Its position in the oracle's list, by density.
    pub index: f64,
    /// T (K), ρ (mol/m³) and p (Pa).
    pub t: f64,
    pub rho: f64,
    pub p: f64,
    /// The oracle's `stable` flag (not asserted: map 04 #14).
    pub stable: bool,
}

/// Every point the oracle returned (`status` ok), in the file's order: by binary, then z1, then density.
pub fn oracle_points() -> Vec<OraclePoint> {
    let (path, text) = fixture!("coolprop-8.0.0/all/mix_crit.csv");
    let table = Fixture::parse(path, text).unwrap();
    (0..table.rows().len())
        .filter(|&row| table.printed(row, "status") == Some("ok"))
        .map(|row| {
            let value = |column: &str| table.value(row, column).unwrap();
            OraclePoint {
                label: table.printed(row, "mixture").unwrap().to_string(),
                z1: value("z1"),
                index: value("index"),
                t: value("T"),
                rho: value("rhomolar"),
                p: value("p"),
                stable: table.printed(row, "stable") == Some("stable"),
            }
        })
        .collect()
}

/// The binary `label` at `z1` on `Parity`, the oracle's data.
pub fn binary(registry: &Registry, label: &str, z1: f64) -> Fluid {
    let names: Vec<&str> = label.split('&').collect();
    phasekit_mix::mixture(registry, &[(names[0], z1), (names[1], 1.0 - z1)], DataSet::Parity).unwrap()
}

/// The mixture model behind `fluid`.
pub fn mixture(fluid: &Fluid) -> &Mixture {
    (fluid.model() as &dyn std::any::Any).downcast_ref::<Mixture>().unwrap()
}

/// The relative offsets in T, ρ and p from `o` of the point of `points` nearest it in |ln T| + |ln ρ|.
pub fn offsets(points: &[CriticalState], o: &OraclePoint) -> Option<[f64; 3]> {
    let distance = |c: &CriticalState| math::ln(c.t / o.t).abs() + math::ln(c.rho / o.rho).abs();
    let nearest = points.iter().min_by(|a, b| distance(a).total_cmp(&distance(b)))?;
    Some([(nearest.t, o.t), (nearest.rho, o.rho), (nearest.p, o.p)].map(|(a, b)| ((a - b) / b).abs()))
}
