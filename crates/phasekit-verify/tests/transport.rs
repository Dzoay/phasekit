//! Transport as the kernel loads and evaluates it (PLAN.md M8; VERIFICATION.md §2): every embedded fluid's transport
//! section decodes on its first transport call (M8.1); the forms evaluate stage by stage from M8.5.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::{BTreeMap, BTreeSet};

use phasekit_core::State;
use phasekit_core::internal::{
    ConductivityModel, FluidRecord, LennardJones, LjSource, TransportRecord, ViscosityModel,
};
use phasekit_core::{
    DataSet, Density, DomainPolicy, Error, FlashOptions, Fluid, Input, LoadError, Phase, Pressure, Prop, Quality,
    Registry, Strategy, Temperature, math,
};
use phasekit_verify::backend;
use phasekit_verify::{DIVERGENCES, Fixture, Kind, Tolerance, ToleranceClass, fixture, register};

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
        ConductivityModel::Hardcoded(_) | ConductivityModel::Methane(_) => "λ hardcoded".into(),
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
    let viscosity = (Prop::Viscosity, transport.viscosity.is_some());
    for (prop, model) in [viscosity, (Prop::Conductivity, transport.conductivity.is_some())] {
        let answer = fluid.prop(&state, prop);
        match answer {
            Err(Error::NoModel { prop: p }) if p == prop && !model => {}
            Ok(value) if model && value.is_finite() && value > 0.0 => {}
            _ if model && lands_later(&answer) => {}
            other => return Err(at(&format!("{prop:?}: {other:?}"))),
        }
    }
    Ok(transport)
}

/// PLAN.md M8.1 (map 10 §8.2): every embedded fluid's transport section decodes, under both datasets. 70 of the 136
/// fluids have no viscosity model and 73 no conductivity model, so the kernel answers `NoModel` for them; the others
/// hold v8.0.0's default forms, as many of each as map 05 §4 counts, and evaluate (a staged viscosity, from M8.5) or say
/// the step that evaluates them.
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

/// Every embedded fluid with a surface-tension fit, with its decoded record.
fn sigma_fits() -> Vec<(FluidRecord, &'static Fluid)> {
    let registry = Registry::embedded().unwrap();
    let records = phasekit_data::FLUIDS.iter().map(|f| phasekit_core::internal::record(registry, f.name).unwrap());
    let fits = records.filter(|r| r.surface_tension.is_some());
    fits.map(|r| {
        let fluid = registry.get(&r.name).unwrap();
        (r, fluid)
    })
    .collect()
}

/// PLAN.md M8.4 (map 13 §3): each of the 108 fits evaluates to Σ aᵢ (1 − T/T_c)ⁿⁱ with its own coefficients and its own
/// T_c, Mulero's (or Okada's, IAPWS's or Kondou's), not the EOS's. Twenty fits state a T_c other than the fluid's
/// published critical temperature (map 05 §3.6), and for each of them the kernel follows the fit's.
#[test]
fn sigma_matches_the_mulero_closed_form() {
    let (mut sources, mut own) = (BTreeMap::new(), 0);
    for (record, fluid) in sigma_fits() {
        let fit = record.surface_tension.as_ref().unwrap();
        let terms = || fit.terms.a.iter().zip(&fit.terms.t);
        let closed = |t: f64, t_c: f64| terms().map(|(a, n)| a * math::powf(1.0 - t / t_c, *n)).sum::<f64>();
        let t_min = record.limits.t_min();
        let t_max = fit.t_c.min(fluid.model().critical_point().unwrap().t);
        for k in 0..4 {
            let t = t_min + (t_max - t_min) * f64::from(k) / 4.0;
            assert_eq!(fluid.model().surface_tension(t), Ok(closed(t, fit.t_c)), "{} at {t} K", record.name);
        }
        // Fourteen published T_c differ from the fit's only in their last digit (`418.09000000000003`): not a different T_c.
        let published = record.critical.unwrap().t;
        if (fit.t_c - published).abs() > 1e-9 {
            own += 1;
            let t = 0.99 * fit.t_c.min(published);
            assert_ne!(closed(t, published), closed(t, fit.t_c), "{}", record.name);
        }
        *sources.entry(fit.source.bibkey.to_string()).or_insert(0) += 1;
    }
    let want = [
        ("IAPWS-SurfaceTension-1994", 1),
        ("Kondou-IJR-2015", 1),
        ("Mulero-JPCRD-2012", 75),
        ("Mulero-JPCRD-2014", 27),
        ("Okada-IJT-1999", 4),
    ];
    assert_eq!(sources, want.map(|(key, n)| (key.to_string(), n)).into_iter().collect());
    assert_eq!(own, 20);
}

/// The oracle's σ files: the all-fluid tier (4 rows per fit) and the core subset's (50 rows each).
const SIGMA_FIXTURES: [(&str, &str); 12] = [
    fixture!("coolprop-8.0.0/all/sigma.csv"),
    fixture!("coolprop-8.0.0/sigma/Ammonia.csv"),
    fixture!("coolprop-8.0.0/sigma/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/sigma/Helium.csv"),
    fixture!("coolprop-8.0.0/sigma/Methanol.csv"),
    fixture!("coolprop-8.0.0/sigma/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/sigma/R1234yf.csv"),
    fixture!("coolprop-8.0.0/sigma/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/sigma/R125.csv"),
    fixture!("coolprop-8.0.0/sigma/R410A.csv"),
    fixture!("coolprop-8.0.0/sigma/Water.csv"),
    fixture!("coolprop-8.0.0/sigma/n-Heptane.csv"),
];

/// PLAN.md M8.4 (VERIFICATION.md §3.5): σ against CoolProp 8.0.0 for all 108 fits at 4 temperatures each over the fit's
/// range (the all-fluid tier), and at 50 for each of the 11 core-subset fluids with one, in class `Prop` against the
/// fit's terms Σ |aᵢ| (1 − T/T_c)ⁿⁱ. A row the oracle cannot evaluate is one the kernel refuses too.
#[test]
fn sigma_matches_oracle() {
    let fits: BTreeMap<String, (FluidRecord, &Fluid)> =
        sigma_fits().into_iter().map(|f| (f.0.name.clone(), f)).collect();
    let (mut rows, mut fluids, mut failures) = (0, BTreeSet::new(), Vec::new());
    for (path, text) in SIGMA_FIXTURES {
        let table = Fixture::parse(path, text).unwrap();
        let file_fluid = path.rsplit('/').next().unwrap().trim_end_matches(".csv");
        for row in 0..table.rows().len() {
            let name = table.printed(row, "fluid").unwrap_or(file_fluid);
            let t = table.value(row, "T").unwrap();
            let (record, fluid) = &fits[name];
            let fit = record.surface_tension.as_ref().unwrap();
            let terms = fit.terms.a.iter().zip(&fit.terms.t);
            let scale: f64 = terms.map(|(a, n)| (a * math::powf(1.0 - t / fit.t_c, *n)).abs()).sum();
            let checked = match (table.printed(row, "status"), fluid.model().surface_tension(t)) {
                (Some("ok"), Ok(sigma)) => {
                    table.check_scaled(row, "sigma", sigma, scale).map(drop).map_err(|e| format!("{e:?}"))
                }
                (Some("ok"), Err(e)) => Err(format!("{e}")),
                (_, got) => got.err().map(drop).ok_or_else(|| "the oracle refuses this row".to_string()),
            };
            failures.extend(checked.err().map(|e| format!("{path}:{row} {name} {t} K: {e}")));
            fluids.insert(name);
            rows += 1;
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!((rows, fluids.len()), (432 + 11 * 50, 108));
}

/// The oracle's transport files: the all-fluid tier (8 states per fluid), the core subset's (200 each), and the probes
/// of the branches no random state reaches (M8.6: Methane's ancillary and near-critical χ; M8.9: Helium's critical
/// term).
const TRANSPORT_FIXTURES: [(&str, &str); 16] = [
    fixture!("coolprop-8.0.0/all/transport.csv"),
    fixture!("coolprop-8.0.0/transport/probes.csv"),
    fixture!("coolprop-8.0.0/transport/Air.csv"),
    fixture!("coolprop-8.0.0/transport/Ammonia.csv"),
    fixture!("coolprop-8.0.0/transport/CarbonDioxide.csv"),
    fixture!("coolprop-8.0.0/transport/HFE143m.csv"),
    fixture!("coolprop-8.0.0/transport/Helium.csv"),
    fixture!("coolprop-8.0.0/transport/Methanol.csv"),
    fixture!("coolprop-8.0.0/transport/Nitrogen.csv"),
    fixture!("coolprop-8.0.0/transport/R1130(E).csv"),
    fixture!("coolprop-8.0.0/transport/R1234yf.csv"),
    fixture!("coolprop-8.0.0/transport/R1234ze(E).csv"),
    fixture!("coolprop-8.0.0/transport/R125.csv"),
    fixture!("coolprop-8.0.0/transport/R410A.csv"),
    fixture!("coolprop-8.0.0/transport/Water.csv"),
    fixture!("coolprop-8.0.0/transport/n-Heptane.csv"),
];

/// Calls `each` with every row of the transport fixtures whose `column` (`eta_model`, `lambda_model`) is `model`: the
/// table, the row, the fluid's name and record, its state at the row's (T, ρ) and its ECS reference fluids, all under
/// `Parity`, the data the oracle has (VERIFICATION.md §6.2).
fn for_transport_rows(
    column: &str,
    model: &str,
    mut each: impl FnMut(&Fixture<'_>, usize, &str, &FluidRecord, &State, &[Fluid]),
) {
    let registry = &Registry::from_embedded(DataSet::Parity).unwrap();
    let mut records: BTreeMap<String, FluidRecord> = BTreeMap::new();
    for (path, text) in TRANSPORT_FIXTURES {
        let table = Fixture::parse(path, text).unwrap();
        let file_fluid = path.rsplit('/').next().unwrap().trim_end_matches(".csv");
        for row in (0..table.rows().len()).filter(|&r| table.printed(r, column) == Some(model)) {
            let name = table.printed(row, "fluid").unwrap_or(file_fluid);
            let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap());
            let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
            let state = registry.get(name).unwrap().flash(input, &FlashOptions::new()).unwrap();
            let record = records
                .entry(name.to_string())
                .or_insert_with(|| phasekit_core::internal::record(registry, name).unwrap());
            let transport = record.transport.as_ref().unwrap();
            let references: Vec<Fluid> =
                transport.references().iter().map(|r| registry.get(r).unwrap().clone()).collect();
            each(&table, row, name, record, &state, &references);
        }
    }
}

/// Whether `result` is the typed error of a form a later step evaluates.
fn lands_later<T>(result: &Result<T, Error>) -> bool {
    matches!(result, Err(Error::Load(LoadError::Format(m))) if m.contains(" lands at M8."))
}

/// PLAN.md M8.5 (map 05 U3): staged viscosity stage by stage against CoolProp's `viscosity_contributions` at the
/// oracle's states (all-fluid tier and core subset), each stage and the total in class `TransportDirect` at the scale of
/// the oracle's total. A staged model with a hardcoded stage (map 05 U10) says it lands at M8.9. On the `libm` backend,
/// Ammonia's η and initial-density term at 8 states are counted, not asserted (user decision LB1, issue #170).
#[test]
fn viscosity_stages_match_contributions() {
    const TEST: &str = "viscosity_stages_match_contributions";
    let (mut checked, mut later, mut failures, mut known) = (BTreeSet::new(), BTreeSet::new(), Vec::new(), 0);
    let columns = [("eta_dilute", 0), ("eta_initial_density", 1), ("eta_residual", 2), ("eta_critical", 3)];
    for_transport_rows("eta_model", "staged", |table, row, name, record, state, _| {
        if table.printed(row, "eta_status") != Some("ok") {
            failures.push(format!("{name} row {row}: the oracle has no viscosity"));
            return;
        }
        let stages = phasekit_core::internal::viscosity_stages(record, state);
        if lands_later(&stages) {
            later.insert(name.to_string());
            return;
        }
        let stages = stages.unwrap();
        let scale = table.value(row, "eta").unwrap();
        let got = [stages.dilute, stages.initial_density, stages.residual, stages.critical, stages.total()];
        for (column, i) in columns.into_iter().chain([("eta", 4)]) {
            match table.check_scaled(row, column, got[i], scale) {
                Err(_) if backend::known(math::BACKEND, TEST, name, column) => known += 1,
                checked => failures.extend(checked.err().map(|e| format!("{name} row {row} {column}: {e:?}"))),
            }
        }
        checked.insert(name.to_string());
    });
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(
        (checked.len(), later.len(), known),
        (34, 0, backend::known_cells(math::BACKEND, TEST)),
        "{checked:?} {later:?}"
    );
}

/// PLAN.md M8.6 (map 05 U4, R6): staged conductivity stage by stage against CoolProp's `conductivity_contributions` at
/// the oracle's states (all-fluid tier and core subset), each stage and the total in class `TransportDirect` at the
/// scale of the oracle's total; the oracle's initial-density column, which staged models do not have, is 0. The
/// Olchowy-Sengers term and `eta0_and_poly` read the fluid's own η, passed in explicitly; where that η is ECS
/// (EthylBenzene, M8.8) the stages are checked in its class, `TransportEcs`. Methane's hard-coded model (Friend, Ely and
/// Ingham 1989) by its total, which the oracle reports as `initial_density`. A model with a hardcoded stage, or whose η
/// a later step evaluates, says the step that evaluates it.
#[test]
fn conductivity_stages_match_contributions() {
    let (mut checked, mut later, mut failures) = (BTreeSet::new(), BTreeSet::new(), Vec::new());
    let columns = [("lambda_dilute", 0), ("lambda_residual", 1), ("lambda_critical", 2), ("lambda_initial_density", 3)];
    for_transport_rows("lambda_model", "staged", |table, row, name, record, state, references| {
        if table.printed(row, "lambda_status") != Some("ok") {
            failures.push(format!("{name} row {row}: the oracle has no conductivity"));
            return;
        }
        let stages = phasekit_core::internal::transport_stages_with(record, state, references).unwrap().1;
        if lands_later(&stages) {
            later.insert(name.to_string());
            return;
        }
        let stages = stages.unwrap();
        let scale = table.value(row, "lambda").unwrap();
        let ecs = matches!(record.transport.as_ref().and_then(|t| t.viscosity.as_ref()), Some(v) if matches!(v.model, ViscosityModel::Ecs(_)));
        let class = if ecs { ToleranceClass::TransportEcs } else { ToleranceClass::TransportDirect };
        let got = [stages.dilute, stages.residual, stages.critical, 0.0];
        for (column, i) in columns {
            let checked = table.check_bound(row, column, got[i], class.bound(scale).unwrap());
            failures.extend(checked.err().map(|e| format!("{name} row {row} {column}: {e:?}")));
        }
        let total = table.check_bound(row, "lambda", stages.total(), class.bound(scale).unwrap());
        failures.extend(total.err().map(|e| format!("{name} row {row} lambda: {e:?}")));
        checked.insert(name.to_string());
    });
    let mut methane = 0;
    for_transport_rows("lambda_model", "hardcoded", |table, row, name, record, state, _| {
        if name == "Methane" {
            let lambda = phasekit_core::internal::conductivity_stages(record, state).unwrap().total();
            let total = table.check_scaled(row, "lambda", lambda, table.value(row, "lambda").unwrap());
            failures.extend(total.err().map(|e| format!("{name} row {row} lambda: {e:?}")));
            methane += 1;
        }
    });
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!((checked.len(), later.len(), methane), (40, 0, 21), "{checked:?} {later:?}");
}

/// PLAN.md M8.7 (map 05 U7): Water's and heavy water's IAPWS releases against CoolProp's totals at the oracle's
/// states (the all-fluid tier, and Water's core subset), η and λ in class `TransportDirect`; λ reads the fluid's own η.
/// CoolProp files a hard-coded model's whole η under `critical` and λ under `initial_density`, so only the totals
/// compare: the releases' tables check the factors (`tests/arbiters.rs`).
#[test]
fn iapws_transport_matches_oracle() {
    let (mut rows, mut failures) = (BTreeMap::new(), Vec::new());
    for_transport_rows("eta_model", "hardcoded", |table, row, name, record, state, _| {
        if name != "Water" && name != "HeavyWater" {
            return;
        }
        let eta = phasekit_core::internal::viscosity_stages(record, state).map(|s| s.total());
        let lambda = phasekit_core::internal::conductivity_stages(record, state).map(|s| s.total());
        for (column, got) in [("eta", eta), ("lambda", lambda)] {
            let checked = got.map_err(|e| e.to_string()).and_then(|got| {
                table.check_scaled(row, column, got, table.value(row, column).unwrap()).map_err(|e| format!("{e:?}"))
            });
            failures.extend(checked.err().map(|e| format!("{name} row {row} {column}: {e}")));
        }
        *rows.entry(name.to_string()).or_insert(0) += 1;
    });
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(rows, BTreeMap::from([("HeavyWater".to_string(), 8), ("Water".to_string(), 208)]));
}

/// PLAN.md M8.8 (map 05 U8; map 10 §8.4): ECS viscosity and conductivity against CoolProp's totals at the oracle's
/// states where it converged, in class `TransportEcs`, through the registry under `Parity`, whose handles hold the
/// reference fluids. CoolProp files an ECS model's whole η under `critical` and λ under `initial_density`, so only the
/// totals compare. 202 cells agree. The oracle's failed cells stay `status=err` (14 η and 16 λ, in 16 of the all-fluid
/// tier's 152 ECS states, all gas; map 10 §8.4). Of those 30, phasekit's Newton in ln T₀ and ln ρ₀ finds the conformal
/// state at 9 states' 16 (R11's and R14's supercritical gas, five dilute gases) and refuses the other 14 with
/// `NoConvergence`. DIV-0026's 32 λ cells are exempt (its replacing check is `div_0026`). R124's, R22's, R245fa's and
/// R32's λ, whose η is ρ*_sr-CS, read Chung's Lennard-Jones estimate, as CoolProp does (DIV-0029).
#[test]
fn ecs_matches_oracle_where_the_oracle_converges() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut checked, mut oracle_failed, mut exempt, mut later) = (0, BTreeMap::new(), 0, BTreeSet::new());
    let (mut fluids, mut failures) = (BTreeSet::new(), Vec::new());
    for (path, text) in TRANSPORT_FIXTURES {
        let table = Fixture::parse(path, text).unwrap();
        let file_fluid = path.rsplit('/').next().unwrap().trim_end_matches(".csv");
        let models = [(Prop::Viscosity, "eta", "eta_model"), (Prop::Conductivity, "lambda", "lambda_model")];
        for (row, (prop, column, _)) in (0..table.rows().len())
            .flat_map(|row| models.into_iter().map(move |m| (row, m)))
            .filter(|(row, (_, _, model))| table.printed(*row, model) == Some("ecs"))
        {
            let name = table.printed(row, "fluid").unwrap_or(file_fluid);
            fluids.insert(name);
            let (t, rho) = (table.value(row, "T").unwrap(), table.value(row, "rho").unwrap());
            let fluid = registry.get(name).unwrap();
            let input = Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap());
            let got = fluid.prop(&fluid.flash(input, &FlashOptions::new()).unwrap(), prop);
            if lands_later(&got) {
                later.insert(name);
                continue;
            }
            if table.printed(row, &format!("{column}_status")) != Some("ok") {
                let refused = matches!(got, Err(Error::NoConvergence { strategy: Strategy::Conformal, .. }));
                *oracle_failed.entry(refused).or_insert(0) += 1;
                continue;
            }
            let got = got.unwrap_or_else(|e| panic!("{name} row {row} {column}: {e}"));
            if register::exempt_at(DIVERGENCES, name, Kind::Transport, column, t).is_some() {
                exempt += 1;
                continue;
            }
            let want = table.value(row, column).unwrap();
            let bound = ToleranceClass::TransportEcs.bound(want).unwrap();
            let checked_row = table.check_bound(row, column, got, bound);
            failures.extend(checked_row.err().map(|e| format!("{name} row {row} {column}: {e:?}")));
            checked += 1;
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(fluids.len(), 19);
    assert_eq!(later, BTreeSet::new());
    assert_eq!((checked, exempt), (202, 32));
    assert_eq!(oracle_failed, BTreeMap::from([(false, 16), (true, 14)]), "solved and refused, where the oracle failed");
}

/// PLAN.md M8.8 (map 05 R2, DIV-0026): an ECS conductivity's critical term reads its own q_D. Of the 18 ECS models five
/// have one other than CoolProp's default, 2e9 1/m (R13, R14, R142b, R218, RC318). Near each one's critical point
/// (1.02 T_c, ρ_c) λ is the record's, and with q_D edited to the default only the critical term moves, by more than
/// 1e-3 of λ.
#[test]
fn q_d_is_read() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/transport.csv");
    let table = Fixture::parse(path, text).unwrap();
    let ecs: BTreeSet<&str> = (0..table.rows().len())
        .filter(|&r| table.printed(r, "lambda_model") == Some("ecs"))
        .map(|r| table.printed(r, "fluid").unwrap())
        .collect();
    let mut own_q_d = Vec::new();
    for name in &ecs {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let mut edited = record.clone();
        let transport = edited.transport.as_mut().unwrap();
        let Some(ConductivityModel::Ecs(model)) = transport.conductivity.as_ref().map(|c| &c.model) else { panic!() };
        let (q_d, references) = (model.q_d, [registry.get(&model.reference).unwrap().clone()]);
        if (q_d / 2e9 - 1.0).abs() < 1e-15 {
            continue;
        }
        own_q_d.push(*name);
        transport.apply(phasekit_core::internal::Edit::EcsQd(2e9)).unwrap();
        let fluid = registry.get(name).unwrap();
        let c = fluid.model().critical_point().unwrap();
        let input = Input::dt(Density::molar(c.rho).unwrap(), Temperature::new(1.02 * c.t).unwrap());
        let state = fluid.flash(input, &FlashOptions::new()).unwrap();
        let stages = |r| phasekit_core::internal::transport_stages_with(r, &state, &references).unwrap().1.unwrap();
        let (own, default) = (stages(&record), stages(&edited));
        // The registry's model and the seam's start the solve from critical points equal to rounding.
        let lambda = fluid.prop(&state, Prop::Conductivity).unwrap();
        assert!((lambda / own.total() - 1.0).abs() < 1e-12, "{name}: {lambda} {own:?}");
        assert_eq!((own.dilute, own.residual), (default.dilute, default.residual), "{name}");
        assert!((own.total() / default.total() - 1.0).abs() > 1e-3, "{name}: {own:?} {default:?}");
    }
    assert_eq!((ecs.len(), own_q_d), (18, vec!["R13", "R14", "R142b", "R218", "RC318"]));
}

/// PLAN.md M8.9 (map 05 U9, U10): every whole model CoolProp hard-codes but the IAPWS releases (M8.7) and Methane's
/// conductivity (M8.6), and the ρ*_sr-CS and Chung viscosities, against CoolProp's totals at the oracle's states (the
/// all-fluid tier and the core subset) in class `TransportDirect`, one case per model; the hard-coded stages of staged
/// models are checked stage by stage (`viscosity_stages_match_contributions`, `conductivity_stages_match_contributions`).
/// DIV-0035's 14 Helium λ cells of 3.5-12 K are exempt (its replacing check is `div_0035`). On the `libm` backend,
/// Methanol's η at one state is counted, not asserted (user decision LB1, issue #170).
#[test]
fn hardcoded_models_match_oracle() {
    const TEST: &str = "hardcoded_models_match_oracle";
    let (mut cases, mut failures, mut exempt, mut known) = (BTreeMap::new(), Vec::new(), 0, 0);
    let kinds = [("eta_model", "hardcoded", "eta"), ("eta_model", "rhosr", "eta"), ("eta_model", "chung", "eta")];
    for (column, model, property) in kinds.into_iter().chain([("lambda_model", "hardcoded", "lambda")]) {
        for_transport_rows(column, model, |table, row, name, record, state, references| {
            if matches!(name, "Water" | "HeavyWater") || (property == "lambda" && name == "Methane") {
                return;
            }
            if table.printed(row, &format!("{property}_status")) != Some("ok") {
                failures.push(format!("{name} row {row}: the oracle has no {property}"));
                return;
            }
            if register::exempt_at(DIVERGENCES, name, Kind::Transport, property, state.t()).is_some() {
                exempt += 1;
                return;
            }
            match hardcoded_total(table, row, record, state, references, property) {
                Err(_) if backend::known(math::BACKEND, TEST, name, property) => known += 1,
                checked => failures.extend(checked.err().map(|e| format!("{name} row {row} {property}: {e}"))),
            }
            *cases.entry(format!("{property} {model} {name}")).or_insert(0) += 1;
        });
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!((exempt, known), (14, backend::known_cells(math::BACKEND, TEST)));
    let models: Vec<&str> = cases.keys().map(String::as_str).collect();
    let rhosr = ["R124", "R1234yf", "R1234ze(E)", "R152A", "R22", "R245fa", "R32"].map(|f| format!("eta rhosr {f}"));
    let whole = ["Helium", "Methanol", "R23", "m-Xylene", "o-Xylene", "p-Xylene"].map(|f| format!("eta hardcoded {f}"));
    let chung = ["Cyclopentane", "Isopentane"].map(|f| format!("eta chung {f}"));
    let lambda = ["Helium", "R23"].map(|f| format!("lambda hardcoded {f}"));
    let mut want: Vec<String> = [&chung[..], &whole[..], &rhosr[..], &lambda[..]].concat();
    want.sort();
    assert_eq!(models, want);
}

/// A model's total `property` (`eta`, `lambda`) at a transport row against the oracle's in the row's class: the ratio
/// of the difference to its bound.
fn hardcoded_total(
    table: &Fixture<'_>,
    row: usize,
    record: &FluidRecord,
    state: &State,
    references: &[Fluid],
    property: &str,
) -> Result<f64, String> {
    let (eta, lambda) = phasekit_core::internal::transport_stages_with(record, state, references).unwrap();
    let got = if property == "eta" { eta.map(|s| s.total()) } else { lambda.map(|s| s.total()) };
    got.map_err(|e| e.to_string()).and_then(|got| {
        let scale = table.value(row, property).unwrap();
        table.check_scaled(row, property, got, scale).map_err(|e| format!("{e:?}"))
    })
}

/// PLAN.md M8.9 (map 05 R4): a ρ*_sr-CS record's (ρ s_r)_c is ρ_r R (τα^r_τ − α^r) at its EOS's reducing state: six of
/// the seven within 2e-6 of their current EOS (R1234ze(E)'s 1.2e-6 the largest), R1234yf's stored −54468.48 6.9e-3 from
/// the −54095.50 of the EOS v8.0.0 ships, which replaced the one it was derived from (DIV-0028; both data sets keep the
/// stored value). Every record's crossover is 2, the x CoolProp fixes (map 05 R4), so reading it changes nothing.
#[test]
fn rhosr_critical_matches_the_eos() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (mut fluids, mut stale) = (Vec::new(), Vec::new());
    for name in ["R1234yf", "R1234ze(E)", "R124", "R152A", "R22", "R245fa", "R32"] {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let model = record.transport.as_ref().and_then(|t| t.viscosity.as_ref()).map(|v| &v.model);
        let Some(ViscosityModel::RhoSrCs(rhosr)) = model else { panic!("{name}") };
        assert_eq!(rhosr.x_crossover, 2.0, "{name}");
        let eos = record.clone().compile().unwrap();
        let (t, rho) = (record.eos.t_reducing, record.eos.rho_reducing);
        let a = eos.eos().residual(t, rho, phasekit_core::Order::One);
        let derived = rho * record.eos.gas_constant * (a.get(1, 0).unwrap() - a.get(0, 0).unwrap());
        let off = derived / rhosr.rhosr_critical - 1.0;
        if off.abs() <= 2e-6 { fluids.push(name) } else { stale.push((name, off)) }
    }
    assert_eq!(fluids.len(), 6, "{fluids:?}");
    assert!(matches!(stale[..], [("R1234yf", off)] if (off + 6.85e-3).abs() < 1e-5), "{stale:?}");
}

/// PLAN.md M8.9 (map 05 R17): an ECS conductivity's dilute terms read the Lennard-Jones parameters its own record
/// carries, which datagen takes from the viscosity block as CoolProp does: the block's own (14 fluids) or, where it has
/// none, Chung's estimate from the EOS reducing state, named (R124, R22, R245fa, R32: DIV-0029). Editing the viscosity's
/// parameters leaves λ's dilute terms as they are; editing the ECS record's moves them.
#[test]
fn ecs_lj_parameters_come_from_the_ecs_record() {
    let registry = Registry::from_embedded(DataSet::Parity).unwrap();
    let (path, text) = fixture!("coolprop-8.0.0/all/transport.csv");
    let table = Fixture::parse(path, text).unwrap();
    let ecs: BTreeSet<&str> = (0..table.rows().len())
        .filter(|&r| table.printed(r, "lambda_model") == Some("ecs"))
        .map(|r| table.printed(r, "fluid").unwrap())
        .collect();
    let mut chung = Vec::new();
    for name in &ecs {
        let record = phasekit_core::internal::record(&registry, name).unwrap();
        let transport = record.transport.as_ref().unwrap();
        let Some(ConductivityModel::Ecs(model)) = transport.conductivity.as_ref().map(|c| &c.model) else { panic!() };
        assert_eq!(Some(model.lennard_jones), transport.viscosity.as_ref().map(|v| v.lennard_jones), "{name}");
        if let LjSource::Chung(lj) = model.lennard_jones {
            let estimate = LjSource::chung(record.eos.t_reducing, record.eos.rho_reducing).parameters();
            assert!(transport_rows::same_lj(lj, estimate), "{name}: {lj:?}, not Chung's {estimate:?}");
            chung.push(*name);
        }
    }
    assert_eq!((ecs.len(), chung), (18, vec!["R124", "R22", "R245fa", "R32"]));
    let record = phasekit_core::internal::record(&registry, "R245fa").unwrap();
    let fluid = registry.get("R245fa").unwrap();
    let state = fluid.state(Input::dt(Density::molar(10.0).unwrap(), Temperature::new(350.0).unwrap())).unwrap();
    let references = [registry.get("R134a").unwrap().clone()];
    let dilute = |record: &FluidRecord| {
        let stages = phasekit_core::internal::transport_stages_with(record, &state, &references).unwrap();
        stages.1.unwrap().dilute
    };
    let wider = |lj: LjSource| LjSource::Model(LennardJones { sigma: 1.1 * lj.parameters().sigma, ..lj.parameters() });
    let mut viscosity_edited = record.clone();
    let viscosity = viscosity_edited.transport.as_mut().unwrap().viscosity.as_mut().unwrap();
    viscosity.lennard_jones = wider(viscosity.lennard_jones);
    assert_eq!(dilute(&viscosity_edited), dilute(&record), "the viscosity's parameters are not λ's");
    let mut ecs_edited = record.clone();
    let conductivity = ecs_edited.transport.as_mut().unwrap().conductivity.as_mut().unwrap();
    let ConductivityModel::Ecs(model) = &mut conductivity.model else { panic!() };
    model.lennard_jones = wider(model.lennard_jones);
    assert!(dilute(&ecs_edited) < 0.9 * dilute(&record), "the ECS record's are");
}

/// PLAN.md M8.5: the arbiter rows of M8.3 (`CoolProp-Tests.cpp`'s paper and IAPWS viscosity rows, Helium's excepted)
/// within their printed digits, for every fluid whose viscosity this step evaluates; the friction-theory fluids with
/// paper rows (SF₆, H₂S) among them. The others say the step that evaluates them. A row beyond its printed digits passes
/// only within the `Measured` bound a register entry records for its table (DIV-0021, DIV-0022; VERIFICATION.md §6),
/// and those rows are pinned. The ten rows outside the EOS's range (CO₂ at 100 and 10 000 K, Methanol at 630 K, the
/// xylenes above p_max, and since M8.11 cyclohexane's 300 K liquid at 9.9508 mol/dm³, below its melting line) are
/// evaluated with `DomainPolicy::Extrapolate`, and the 13 at a (T, ρ) inside the dome, where
/// η is `Undefined` (M8.10), at the single phase there, as their papers evaluate them: six saturated densities rounded
/// into the dome (Ammonia's 200 K liquid, 7.4e-4 below ρ′), the xylenes' and cyclohexane's six dilute checks at 300 and
/// 400 K, up to 99 times ρ″, and cyclohexane's 500 K liquid, 4.6 % below ρ′.
#[test]
fn viscosity_paper_rows() {
    let rows = paper_rows(Prop::Viscosity, ".eta.csv");
    assert!(rows.failures.is_empty(), "{:#?}", rows.failures);
    let dome = ["Ammonia 86", "Ammonia 87", "CycloHexane 231", "CycloHexane 235", "R123 145", "R123 147", "R23 206"];
    let xylenes = ["m-Xylene 250", "m-Xylene 254", "n-Propane 46", "o-Xylene 263", "o-Xylene 267", "p-Xylene 276"];
    assert_eq!(rows.in_dome, [&dome[..], &xylenes[..]].concat());
    let checked = &rows.checked;
    assert_eq!((checked.get("SulfurHexafluoride"), checked.get("HydrogenSulfide")), (Some(&6), Some(&4)));
    assert_eq!((checked.get("Water"), checked.get("HeavyWater")), (Some(&17), Some(&3)));
    assert_eq!((checked.len(), checked.values().sum::<usize>(), rows.later), (29, 157, 0), "{checked:?}");
    let outside =
        ["CarbonDioxide 133", "CarbonDioxide 135", "CycloHexane 233", "Methanol 218", "Methanol 219", "m-Xylene 252"];
    assert_eq!(
        rows.extrapolated,
        [&outside[..], &["m-Xylene 259", "o-Xylene 265", "o-Xylene 269", "o-Xylene 272"]].concat()
    );
    let registered = [
        (("DIV-0021", "R125"), 1),
        (("DIV-0021", "SulfurHexafluoride"), 6),
        (("DIV-0022", "Ammonia"), 1),
        (("DIV-0022", "R123"), 1),
        (("DIV-0022", "n-Octane"), 1),
        (("DIV-0022", "n-Propane"), 4),
        (("DIV-0031", "Benzene"), 1),
        (("DIV-0031", "Methanol"), 2),
        (("DIV-0031", "n-Heptane"), 2),
        (("DIV-0033", "Ethane"), 2),
        (("DIV-0034", "CarbonDioxide"), 1),
        (("DIV-0034", "R23"), 2),
        (("IAPWS-R4-84(2007)", "HeavyWater"), 3),
    ];
    assert_eq!(rows.beyond(), registered, "rows beyond their printed digits, by entry");
}

/// PLAN.md M8.6: the arbiter rows of M8.3 among CoolProp's 141 active conductivity rows (Helium's excepted) within
/// their printed digits, for every fluid whose conductivity this step evaluates, as `viscosity_paper_rows` does η's,
/// the two inside the dome (R123's and R23's 180 K liquids, saturated densities rounded into it) at the single phase
/// there.
#[test]
fn conductivity_paper_rows() {
    let rows = paper_rows(Prop::Conductivity, ".lambda.csv");
    assert!(rows.failures.is_empty(), "{:#?}", rows.failures);
    assert_eq!(rows.in_dome, ["R123 441", "R23 499"]);
    let checked = &rows.checked;
    assert_eq!((checked.get("Water"), checked.get("HeavyWater")), (Some(&12), Some(&3)));
    assert_eq!((checked.len(), checked.values().sum::<usize>(), rows.later), (27, 105, 0), "{checked:?}");
    assert_eq!(rows.extrapolated, Vec::<String>::new());
    let registered = [
        (("DIV-0023", "Air"), 1),
        (("DIV-0023", "Argon"), 1),
        (("DIV-0023", "Nitrogen"), 1),
        (("DIV-0023", "Oxygen"), 1),
        (("DIV-0023", "SulfurHexafluoride"), 1),
        (("DIV-0023", "n-Nonane"), 1),
        (("DIV-0024", "Methane"), 3),
        (("DIV-0025", "n-Pentane"), 1),
        (("DIV-0027", "EthylBenzene"), 1),
        (("DIV-0027", "o-Xylene"), 1),
        (("DIV-0032", "Cyclopentane"), 1),
        (("DIV-0032", "Isopentane"), 1),
        (("DIV-0032", "n-Heptane"), 3),
        (("DIV-0032", "n-Hexane"), 1),
        (("DIV-0033", "Ethane"), 2),
        (("DIV-0034", "R123"), 1),
        (("DIV-0034", "R23"), 2),
        (("IAPWS-R4-84(2007)", "HeavyWater"), 3),
    ];
    assert_eq!(rows.beyond(), registered, "rows beyond their printed digits, by entry");
}

/// PLAN.md M8.9 (map 05 U9, U10): every hard-coded form with arbiter rows among M8.3's is checked against them, each row
/// within its printed digits or a register entry's bound (the two paper-row tests list those): the xylenes and
/// cyclohexane, Shan's R23, Friend's ethane, Huber 2016's and Laesecke and Muzny's CO₂, Laesecke's R123 critical term,
/// Xiang's methanol, Muzny's hydrogen and the higher-order terms of n-hexane, n-heptane, toluene and benzene, with
/// cyclopentane's and isopentane's conductivity, whose critical term reads the Chung viscosity. Helium's rows do not
/// arbitrate (ROT-124), Ammonia's conductivity rows are REFPROP's, and no ρ*_sr-CS fluid has rows.
#[test]
fn hardcoded_models_match_their_paper_tables() {
    let (eta, lambda) = (paper_rows(Prop::Viscosity, ".eta.csv"), paper_rows(Prop::Conductivity, ".lambda.csv"));
    assert!(eta.failures.is_empty() && lambda.failures.is_empty(), "{:#?} {:#?}", eta.failures, lambda.failures);
    let count = |rows: &PaperRows, fluid: &str| rows.checked.get(fluid).copied().unwrap_or(0);
    let models = [
        ("m-Xylene", 11, 2),
        ("o-Xylene", 11, 2),
        ("p-Xylene", 9, 2),
        ("CycloHexane", 9, 0),
        ("R23", 3, 3),
        ("Ethane", 3, 4),
        ("CarbonDioxide", 9, 4),
        ("R123", 4, 4),
        ("Methanol", 4, 4),
        ("Hydrogen", 0, 6),
        ("n-Hexane", 6, 4),
        ("n-Heptane", 6, 4),
        ("Toluene", 6, 0),
        ("Benzene", 6, 0),
        ("Cyclopentane", 0, 2),
        ("Isopentane", 0, 2),
    ];
    let counts: Vec<_> = models.iter().map(|(f, _, _)| (*f, count(&eta, f), count(&lambda, f))).collect();
    assert_eq!(counts, models);
    let none = ["Helium", "Ammonia", "R1234yf", "R1234ze(E)", "R124", "R152A", "R22", "R245fa", "R32"];
    assert_eq!(none.map(|f| count(&lambda, f) + if f == "Ammonia" { 0 } else { count(&eta, f) }), [0; 9]);
}

/// What the arbiter rows of one property say (PLAN.md M8.5, M8.6).
#[derive(Default)]
struct PaperRows {
    /// Rows checked, per fluid.
    checked: BTreeMap<&'static str, usize>,
    /// Rows whose form a later step evaluates.
    later: usize,
    /// Rows outside the EOS's range, "fluid line".
    extrapolated: Vec<String>,
    /// Rows at a (T, ρ) inside the dome, evaluated at the single phase there (M8.10), "fluid line".
    in_dome: Vec<String>,
    /// Rows beyond their printed digits, within the bound of (register entry or restated release, fluid).
    beyond: Vec<(&'static str, &'static str)>,
    /// What failed.
    failures: Vec<String>,
}

impl PaperRows {
    /// The rows beyond their printed digits, counted per (register entry, fluid).
    fn beyond(&self) -> Vec<((&'static str, &'static str), usize)> {
        let counts = self.beyond.iter().fold(BTreeMap::new(), |mut n, key| {
            *n.entry(*key).or_insert(0) += 1;
            n
        });
        counts.into_iter().collect()
    }
}

/// Every arbiter row of `prop` in M8.3's files ending in `suffix`, evaluated and checked.
fn paper_rows(prop: Prop, suffix: &str) -> PaperRows {
    let registry = Registry::embedded().unwrap();
    let mut rows = PaperRows::default();
    for ((path, text), _) in transport_rows::ROWS.iter().filter(|((p, _), _)| p.ends_with(suffix)) {
        let table = Fixture::parse(path, text).unwrap();
        let fluid = path.split('/').nth(1).unwrap();
        for row in (0..table.rows().len()).filter(|&r| arbitrates(fluid, table.printed(r, "provenance").unwrap())) {
            let line = table.value(row, "line").unwrap();
            let (outside, in_dome, outcome) = paper_row(registry.get(fluid).unwrap(), path, &table, row, prop);
            rows.extrapolated.extend(outside.then(|| format!("{fluid} {line}")));
            rows.in_dome.extend(in_dome.then(|| format!("{fluid} {line}")));
            match outcome {
                Ok(None) => rows.later += 1,
                Ok(Some(id)) => {
                    *rows.checked.entry(fluid).or_insert(0) += 1;
                    rows.beyond.extend(id.map(|id| (id, fluid)));
                }
                Err(e) => rows.failures.push(format!("{path} line {line}: {e}")),
            }
        }
    }
    rows
}

/// One arbiter row: whether it lies outside the EOS's range (evaluated only on request) and inside the dome, and `None`
/// when its form lands later, else the register entry whose bound it needs beyond its printed digits.
fn paper_row(
    fluid: &Fluid,
    path: &str,
    table: &Fixture<'_>,
    row: usize,
    prop: Prop,
) -> (bool, bool, Result<Option<Option<&'static str>>, String>) {
    let mut value = paper_row_value(fluid, table, row, DomainPolicy::Enforce, prop);
    // A row outside the EOS's range, inside the correlation's.
    let outside = matches!(value, Err(Error::Domain(_)));
    if outside {
        value = paper_row_value(fluid, table, row, DomainPolicy::Extrapolate, prop);
    }
    let in_dome = matches!(value, Ok((_, true)));
    (outside, in_dome, paper_row_check(path, table, row, value.map(|(v, _)| v)))
}

/// `value` against its row: `None` when its form lands later, else the register entry, or the release whose cell the
/// row restates, whose bound it needs beyond its printed digits.
fn paper_row_check(
    path: &str,
    table: &Fixture<'_>,
    row: usize,
    eta: Result<f64, Error>,
) -> Result<Option<Option<&'static str>>, String> {
    if lands_later(&eta) {
        return Ok(None);
    }
    let eta = eta.map_err(|e| e.to_string())?;
    let Err(error) = table.check(row, "value", eta) else { return Ok(Some(None)) };
    let want = table.value(row, "value").unwrap();
    if let Some((release, bound)) = restated_cell(path, table, row)
        && (eta - want).abs() <= bound
    {
        return Ok(Some(Some(release)));
    }
    match registered_bound(path) {
        Some((id, bound)) if (eta / want - 1.0).abs() <= bound => Ok(Some(Some(id))),
        _ => Err(format!("{error:?}")),
    }
}

/// The release cell a CoolProp row restates, with half a unit of its last printed digit as a bound on the row: the
/// heavy-water rows (`CoolProp-Tests.cpp:289-291, 570-572`) print IAPWS R4-84(2007)'s Table A5 μ̄ and Table B4 λ̄ times
/// μ* = 55.2651 µPa s and λ* = 0.742128 mW/(m K) to 17 and 18 digits, which the release's ten decimals do not hold. The
/// row must be that cell times its reference value, to rounding.
fn restated_cell(path: &str, table: &Fixture<'_>, row: usize) -> Option<(&'static str, f64)> {
    let (release, column, scale) = match path {
        "paper/HeavyWater/CoolProp-Tests.eta.csv" => ("paper/HeavyWater/IAPWS-R4-84-2007.A5.csv", "mubar", 55.2651e-6),
        "paper/HeavyWater/CoolProp-Tests.lambda.csv" => {
            ("paper/HeavyWater/IAPWS-R4-84-2007.B4.csv", "lambdabar", 0.742128e-3)
        }
        _ => return None,
    };
    let text = phasekit_verify::arbiters::TABLES.iter().find(|(p, _)| *p == release)?.1;
    let cells = Fixture::parse(release, text).unwrap();
    let (t, rho) = (table.value(row, "T")? / 643.847, table.value(row, "x")? / 358.0);
    let at = |r: usize, c: &str, x: f64| (cells.value(r, c).unwrap() - x).abs() < 1e-9;
    let cell = (0..cells.rows().len()).find(|&r| at(r, "Tbar", t) && at(r, "rhobar", rho))?;
    let (value, printed) = (cells.value(cell, column)?, cells.printed(cell, column)?);
    assert!((table.value(row, "value")? / (value * scale) - 1.0).abs() < 1e-15, "{path} row {row}");
    let Some(Tolerance::Absolute(half)) = phasekit_verify::from_printed(printed) else { return None };
    Some(("IAPWS-R4-84(2007)", half * scale))
}

/// The `Measured` bound a register entry records for the table `path` (VERIFICATION.md §6.2), with the entry's id.
fn registered_bound(path: &str) -> Option<(&'static str, f64)> {
    DIVERGENCES.iter().find_map(|d| match d.table_tolerances.iter().find(|(f, _)| *f == path) {
        Some((_, Tolerance::Relative(bound))) => Some((d.id, *bound)),
        _ => None,
    })
}

/// `prop` at a CoolProp test row's inputs (T and a molar or mass density, a quality, or a pressure) under `domain`, and
/// whether they lie inside the dome, where it is the single phase's there ([`transport_rows::dome_side`]).
fn paper_row_value(
    fluid: &Fluid,
    table: &Fixture<'_>,
    row: usize,
    domain: DomainPolicy,
    prop: Prop,
) -> Result<(f64, bool), Error> {
    let t = Temperature::new(table.value(row, "T").unwrap())?;
    let x = table.value(row, "x").unwrap();
    let input = match table.printed(row, "x_kind").unwrap() {
        "Dmolar" => Input::dt(Density::molar(x)?, t),
        "Dmass" | "D" => Input::dt(Density::mass(x)?, t),
        "Q" => Input::qt(Quality::new(x)?, t),
        _ => Input::pt(Pressure::new(x)?, t),
    };
    let options = FlashOptions::new().with_domain(domain);
    // CoolProp's zero-density rows (1e-5 kg/m³) outside the range: below the triple point no VLE can label them.
    let dilute = domain == DomainPolicy::Extrapolate && x <= 1e-3;
    let options = if dilute { options.with_phase(Phase::Gas) } else { options };
    let state = fluid.flash(input, &options)?;
    let side = transport_rows::dome_side(fluid, &state);
    let state = match side {
        Some(phase) => fluid.flash(input, &options.with_phase(phase))?,
        None => state,
    };
    Ok((fluid.prop(&state, prop)?, side.is_some()))
}
