//! Datagen's tests, one module per stage: the pinned files read ([`files`]), each EOS mapped into core's record
//! ([`mapping`]), the blobs ([`blobs`]), the ideal-gas kinds as converted ([`ideal`]), the caloric curves
//! ([`caloric_curves`]) and the transport blocks ([`transport_blocks`]); the fixtures they share are here.

use super::*;
use mirror::{IdealBlock, Num, ResidualBlock};
use phasekit_core::internal::{Environmental, EosRecord, FluidRecord, IdealTerm};
use phasekit_core::{Jet4, Limits, Order, Real, math};

mod blobs;
mod caloric_curves;
mod files;
mod ideal;
mod mapping;
mod mixtures;
mod transport_blocks;

fn sources() -> Vec<Source> {
    load(&Repo::locate()).unwrap()
}

/// The text of one pinned fluid file.
fn text(file: &str) -> String {
    Repo::locate().read(&format!("{FLUIDS_DIR}/{file}")).unwrap()
}

/// What a pinned file's conductivity forms read of the rest of it (M8.6).
fn from_fluid(file: &str) -> transport::FromFluid {
    transport::FromFluid::of(&serde_json::from_str(&text(file)).unwrap()).unwrap()
}

/// An edit of a parsed fluid file.
type Edit = fn(&mut serde_json::Value);

/// A pinned file with one edit applied to its parsed JSON, written back as JSON (numbers keep their literal kind).
fn edited(file: &str, edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&text(file)).unwrap();
    edit(&mut value);
    serde_json::to_string(&value).unwrap()
}

/// Every fluid's record, by name.
fn records() -> Vec<(Source, FluidRecord)> {
    sources()
        .into_iter()
        .map(|s| {
            let r = record::to_record(&s).unwrap();
            (s, r)
        })
        .collect()
}

fn record_of(file: &str, text: &str) -> Result<FluidRecord, String> {
    record::to_record(&parse(file, text)?)
}

/// How far, in temperature, `model`'s melting curve misses the EOS's triple point (`t_triple`, `p_triple`): the curve
/// from its lowest-pressure end (T_a, p_min), followed at its slope there, reaches p_triple at T_a + (p_triple −
/// p_min)/slope, which is this far from t_triple. Read through the public curve only: the curve's pressure bounds are
/// the ones its `Domain` errors name.
fn triple_offset(model: &dyn phasekit_core::ThermoModel, t_triple: f64, p_triple: f64) -> f64 {
    use phasekit_core::{DomainError, Error};
    let bound = |p: f64| match model.melting_temperature(p) {
        Err(Error::Domain(DomainError::BelowMinPressure { p_min, .. })) => p_min,
        Err(Error::Domain(DomainError::AboveMaxPressure { p_max, .. })) => p_max,
        other => panic!("{other:?}"),
    };
    let (p_min, p_max) = (bound(f64::MIN_POSITIVE), bound(f64::MAX));
    let t_a = model.melting_temperature(p_min).unwrap();
    let dp = 1e-6 * p_max;
    let slope = dp / (model.melting_temperature(p_min + dp).unwrap() - t_a);
    t_a + (p_triple - p_min) / slope - t_triple
}

/// Rot: ROT-048 (map 02 §6 row 5; PLAN.md M8.11). Every melting curve meets its EOS's triple point (T_triple, or the
/// saturation curve's lowest T if higher, and the EOS's saturation pressure there) within 0.05 K in temperature ([`triple_offset`]): the step's bound, since the
/// JSON carries no fit's own tolerance; the others meet it within 0.023 K (Krypton, then CarbonMonoxide). Helium is
/// exempt, having no solid-liquid-vapour triple point (its curve starts at 2.2 MPa). The waivers, each pinned at its
/// measured offset (the curve's T at the triple pressure minus the triple point's): Hydrogen −7.76 K and OrthoHydrogen
/// −7.81 K (DIV-0036); Deuterium and its ortho and para forms +0.995 K, their fit starting at 19.72 K, a kelvin above
/// their triple point; Ethanol −0.73 K, its fit's T₀ being 158.37 K, not the EOS's 159.1 K; Isopentane −0.15 K, its
/// fit's T₀ being 112.5 K and p₀ 0, not the 112.65 K triple point.
#[test]
fn melting_meets_the_triple_pressure() {
    use phasekit_core::{Input, Quality, Temperature};
    let waivers = [
        ("Hydrogen", -7.760),
        ("OrthoHydrogen", -7.811),
        ("Deuterium", 0.995),
        ("OrthoDeuterium", 0.995),
        ("ParaDeuterium", 0.995),
        ("Ethanol", -0.730),
        ("Isopentane", -0.150),
    ];
    // The generated records, through the embedded registry, which attaches their saturation curves (the `datagen`
    // gate keeps its blobs those of a fresh generation).
    let registry = phasekit_core::Registry::from_embedded(phasekit_core::DataSet::Parity).unwrap();
    let (mut met, mut waived) = (Vec::new(), Vec::new());
    for (_, record) in records().into_iter().filter(|(_, r)| !r.melting.is_empty()) {
        let name = record.name.clone();
        // The domain's lowest temperature: Ethanol's saturation curve starts 0.1 K above its 159.0 K triple point.
        let t_triple = record.limits.t_triple().unwrap().max(record.limits.t_min());
        let fluid = registry.get(&name).unwrap();
        let p_triple =
            fluid.state(Input::qt(Quality::new(0.0).unwrap(), Temperature::new(t_triple).unwrap())).unwrap().p();
        let off = triple_offset(fluid.model(), t_triple, p_triple);
        match waivers.iter().find(|(w, _)| *w == name) {
            Some((_, measured)) => {
                assert!((off - measured).abs() < 1e-3, "{name}: {off} K, waived at {measured}");
                waived.push(name);
            }
            None if name == "Helium" => {}
            None => {
                assert!(off.abs() <= 0.05, "{name}: the melting curve misses the triple point by {off} K");
                met.push((name, off));
            }
        }
    }
    assert_eq!(waived.len(), waivers.len());
    assert_eq!(met.len(), 30 - 1 - waivers.len(), "{met:?}");
    let worst = met.iter().map(|(_, off)| off.abs()).fold(0.0, f64::max);
    assert!(worst < 0.025, "{met:?}");
}
