//! Transport as the kernel loads and evaluates it (PLAN.md M8; VERIFICATION.md §2): every embedded fluid's transport
//! section decodes on its first transport call (M8.1); the forms evaluate stage by stage from M8.5.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::{BTreeMap, BTreeSet};

use phasekit_core::internal::{ConductivityModel, TransportRecord, ViscosityModel};
use phasekit_core::{DataSet, Density, Error, FlashOptions, Input, LoadError, Prop, Registry, Temperature};
use phasekit_verify::Fixture;

#[path = "common/transport_rows.rs"]
mod transport_rows;

/// Each model's form, and each ECS model's reference fluid.
fn forms(record: &TransportRecord, into: &mut BTreeMap<String, usize>) {
    let viscosity = record.viscosity.as_ref().map(|v| match &v.model {
        ViscosityModel::Staged(staged) if staged.initial.is_some() => "η staged, initial density".to_string(),
        ViscosityModel::Staged(_) => "η staged".into(),
        ViscosityModel::Ecs(ecs) => format!("η ECS → {}", ecs.reference),
        ViscosityModel::RhoSrCs(_) => "η rhosr-CS".into(),
        ViscosityModel::Chung(_) => "η Chung".into(),
        ViscosityModel::Hardcoded(_) => "η hardcoded".into(),
    });
    let conductivity = record.conductivity.as_ref().map(|c| match &c.model {
        ConductivityModel::Staged(_) => "λ staged".to_string(),
        ConductivityModel::Ecs(ecs) => format!("λ ECS → {}", ecs.reference),
        ConductivityModel::Hardcoded(_) => "λ hardcoded".into(),
    });
    viscosity.into_iter().chain(conductivity).for_each(|form| *into.entry(form).or_default() += 1);
}

/// `name`'s decoded transport record, once the kernel has answered for it as the record says: `NoModel` for a property
/// without a model, else the step that evaluates the model.
fn answered(reg: &Registry, name: &str) -> Result<TransportRecord, String> {
    let at = |e: &dyn std::fmt::Display| format!("{name}: {e}");
    let fluid = reg.get(name).map_err(|e| at(&e))?;
    let record = phasekit_core::internal::record(reg, name).map_err(|e| at(&e))?;
    let transport = record.transport.ok_or_else(|| at(&"transport not decoded"))?;
    let t = (1.05 * record.eos.t_reducing).min(record.limits.t_max());
    let input = Input::dt(Density::molar(1e-3 * record.eos.rho_reducing).unwrap(), Temperature::new(t).unwrap());
    let state = fluid.flash(input, &FlashOptions::new()).map_err(|e| at(&e))?;
    let viscosity = (Prop::Viscosity, transport.viscosity.is_some(), "M8.5");
    for (prop, model, step) in [viscosity, (Prop::Conductivity, transport.conductivity.is_some(), "M8.6")] {
        match fluid.prop(&state, prop) {
            Err(Error::NoModel { prop: p }) if p == prop && !model => {}
            Err(Error::Load(LoadError::Format(m))) if model && m.ends_with(&format!("lands at {step}")) => {}
            other => return Err(at(&format!("{prop:?}: {other:?}"))),
        }
    }
    Ok(transport)
}

/// PLAN.md M8.1 (map 10 §8.2): every embedded fluid's transport section decodes, under both datasets. 70 of the 136
/// fluids have no viscosity model and 73 no conductivity model, so the kernel answers `NoModel` for them; the others
/// hold v8.0.0's default forms, as many of each as map 05 §4 counts, and report that evaluating them lands at M8.5
/// (viscosity) and M8.6 (conductivity).
#[test]
fn all_transport_blocks_decode() {
    let want: BTreeMap<String, usize> = [
        ("η staged", 14),
        ("η staged, initial density", 20),
        ("η ECS → Propane", 8),
        ("η ECS → R134a", 6),
        ("η ECS → Nitrogen", 1),
        ("η rhosr-CS", 7),
        ("η Chung", 2),
        ("η hardcoded", 8),
        ("λ staged", 40),
        ("λ ECS → Propane", 9),
        ("λ ECS → R134a", 8),
        ("λ ECS → Nitrogen", 1),
        ("λ hardcoded", 5),
    ]
    .into_iter()
    .map(|(form, n)| (form.to_string(), n))
    .collect();
    for set in [DataSet::Parity, DataSet::Corrected] {
        let reg = Registry::from_embedded(set).unwrap();
        let (mut no_model, mut found) = ([0, 0], BTreeMap::new());
        for f in phasekit_data::FLUIDS {
            let transport = answered(&reg, f.name).unwrap();
            no_model[0] += usize::from(transport.viscosity.is_none());
            no_model[1] += usize::from(transport.conductivity.is_none());
            forms(&transport, &mut found);
        }
        assert_eq!(no_model, [70, 73], "{set:?}");
        assert_eq!(found, want, "{set:?}");
    }
}

/// Where a CoolProp transport row's value comes from (map 10 §8.1): a correlation paper's check value, an IAPWS
/// release's, measured data, REFPROP's output, CoolProp's own (map 10 R6), or no source at all.
const PROVENANCES: [&str; 6] = ["paper", "iapws", "experiment", "refprop", "self", "none"];

/// Whether a row arbitrates: a paper's or an IAPWS release's value, never Helium's. CoolProp's helium correlations
/// follow REFPROP's source code ("magical coefficients", map 05 §3.8), so no paper can arbitrate them (ROT-124).
fn arbitrates(fluid: &str, provenance: &str) -> bool {
    matches!(provenance, "paper" | "iapws") && fluid != "Helium"
}

/// The source lines of the two arrays: viscosity, conductivity.
const ARRAYS: [(&str, u32, u32); 2] = [("eta", 44, 301), ("lambda", 347, 581)];

/// The arbiter rows per fluid and property, PLAN.md M8.3's record (262 of the 318).
const ARBITER_ROWS: [(&str, &str, usize); 56] = [
    ("Air", "eta", 6),
    ("Air", "lambda", 6),
    ("Ammonia", "eta", 4),
    ("Argon", "eta", 6),
    ("Argon", "lambda", 6),
    ("Benzene", "eta", 6),
    ("CarbonDioxide", "eta", 9),
    ("CarbonDioxide", "lambda", 4),
    ("CycloHexane", "eta", 9),
    ("Cyclopentane", "lambda", 2),
    ("Ethane", "eta", 3),
    ("Ethane", "lambda", 4),
    ("Ethanol", "lambda", 4),
    ("EthylBenzene", "eta", 1),
    ("EthylBenzene", "lambda", 2),
    ("HeavyWater", "eta", 3),
    ("HeavyWater", "lambda", 3),
    ("Hydrogen", "lambda", 6),
    ("HydrogenSulfide", "eta", 4),
    ("Isopentane", "lambda", 2),
    ("Methane", "lambda", 4),
    ("Methanol", "eta", 4),
    ("Methanol", "lambda", 4),
    ("Nitrogen", "eta", 6),
    ("Nitrogen", "lambda", 6),
    ("Oxygen", "eta", 6),
    ("Oxygen", "lambda", 6),
    ("R123", "eta", 4),
    ("R123", "lambda", 4),
    ("R125", "eta", 2),
    ("R23", "eta", 3),
    ("R23", "lambda", 3),
    ("SulfurHexafluoride", "eta", 6),
    ("SulfurHexafluoride", "lambda", 6),
    ("Toluene", "eta", 6),
    ("Water", "eta", 17),
    ("Water", "lambda", 12),
    ("m-Xylene", "eta", 11),
    ("m-Xylene", "lambda", 2),
    ("n-Decane", "eta", 1),
    ("n-Dodecane", "eta", 2),
    ("n-Dodecane", "lambda", 3),
    ("n-Heptane", "eta", 6),
    ("n-Heptane", "lambda", 4),
    ("n-Hexane", "eta", 6),
    ("n-Hexane", "lambda", 4),
    ("n-Nonane", "eta", 1),
    ("n-Nonane", "lambda", 1),
    ("n-Octane", "eta", 1),
    ("n-Octane", "lambda", 1),
    ("n-Pentane", "lambda", 2),
    ("n-Propane", "eta", 4),
    ("o-Xylene", "eta", 11),
    ("o-Xylene", "lambda", 2),
    ("p-Xylene", "eta", 9),
    ("p-Xylene", "lambda", 2),
];

/// Rot: ROT-124. PLAN.md M8.3 (map 10 §8.1, R6): every one of the 318 active transport rows of CoolProp's tests is
/// transcribed once, with its source line, the source its block cites and its provenance; only `paper` and `iapws`
/// rows arbitrate, never Helium's, and how many each fluid has is recorded. 56 rows do not arbitrate: 26 computed by
/// REFPROP, 2 by CoolProp itself, 2 measured, 6 without a source, and Helium's 20.
#[test]
fn transport_rows_carry_their_provenance() {
    let (mut lines, mut classes, mut arbiters) = (BTreeSet::new(), BTreeMap::new(), BTreeMap::new());
    for ((path, text), _) in transport_rows::ROWS {
        let table = Fixture::parse(path, text).unwrap();
        let fluid = path.split('/').nth(1).unwrap();
        let prop = path.rsplit('.').nth(1).unwrap();
        let (_, first, last) = ARRAYS.iter().find(|(p, ..)| *p == prop).unwrap();
        for row in 0..table.rows().len() {
            let line = table.value(row, "line").unwrap() as u32;
            assert!((*first..=*last).contains(&line) && lines.insert(line), "{path}: line {line}");
            let provenance = table.printed(row, "provenance").unwrap();
            assert!(PROVENANCES.contains(&provenance), "{path}: line {line}: provenance `{provenance}`");
            *classes.entry((prop, provenance)).or_insert(0) += 1;
            if arbitrates(fluid, provenance) {
                *arbiters.entry((fluid, prop)).or_insert(0) += 1;
            }
        }
    }
    let per_array = |prop: &str| classes.iter().filter(|((p, _), _)| *p == prop).map(|(_, n)| n).sum::<usize>();
    assert_eq!((lines.len(), per_array("eta"), per_array("lambda")), (318, 177, 141));
    let want_classes = [
        (("eta", "experiment"), 2),
        (("eta", "iapws"), 20),
        (("eta", "paper"), 140),
        (("eta", "refprop"), 14),
        (("eta", "self"), 1),
        (("lambda", "iapws"), 3),
        (("lambda", "none"), 6),
        (("lambda", "paper"), 119),
        (("lambda", "refprop"), 12),
        (("lambda", "self"), 1),
    ];
    assert_eq!(classes, BTreeMap::from(want_classes));
    let want: BTreeMap<_, _> = ARBITER_ROWS.iter().map(|&(fluid, prop, n)| ((fluid, prop), n)).collect();
    assert_eq!(arbiters, want);
    assert_eq!(arbiters.values().sum::<usize>(), 262);
}
