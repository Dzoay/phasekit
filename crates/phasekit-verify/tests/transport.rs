//! Transport as the kernel loads and evaluates it (PLAN.md M8; VERIFICATION.md §2): every embedded fluid's transport
//! section decodes on its first transport call (M8.1); the forms evaluate stage by stage from M8.5.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::{BTreeMap, BTreeSet};

use phasekit_core::State;
use phasekit_core::internal::{ConductivityModel, FluidRecord, TransportRecord, ViscosityModel};
use phasekit_core::{
    DataSet, Density, DomainPolicy, Error, FlashOptions, Fluid, Input, LoadError, Phase, Pressure, Prop, Quality,
    Registry, Temperature, math,
};
use phasekit_verify::{DIVERGENCES, Fixture, Tolerance, fixture};

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

/// The oracle's transport files: the all-fluid tier (8 states per fluid) and the core subset's (200 each).
const TRANSPORT_FIXTURES: [(&str, &str); 15] = [
    fixture!("coolprop-8.0.0/all/transport.csv"),
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
/// table, the row, the fluid's name and record, and the fluid's state at the row's (T, ρ).
fn for_transport_rows(
    column: &str,
    model: &str,
    mut each: impl FnMut(&Fixture<'_>, usize, &str, &FluidRecord, &State),
) {
    let registry = Registry::embedded().unwrap();
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
            each(&table, row, name, record, &state);
        }
    }
}

/// Whether `result` is the typed error of a form a later step evaluates.
fn lands_later<T>(result: &Result<T, Error>) -> bool {
    matches!(result, Err(Error::Load(LoadError::Format(m))) if m.contains(" lands at M8."))
}

/// PLAN.md M8.5 (map 05 U3): staged viscosity stage by stage against CoolProp's `viscosity_contributions` at the
/// oracle's states (all-fluid tier and core subset), each stage and the total in class `TransportDirect` at the scale of
/// the oracle's total. A staged model with a hardcoded stage (map 05 U10) says it lands at M8.9.
#[test]
fn viscosity_stages_match_contributions() {
    let (mut checked, mut later, mut failures) = (BTreeSet::new(), BTreeSet::new(), Vec::new());
    let columns = [("eta_dilute", 0), ("eta_initial_density", 1), ("eta_residual", 2)];
    for_transport_rows("eta_model", "staged", |table, row, name, record, state| {
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
        let got = [stages.dilute, stages.initial_density, stages.residual];
        for (column, i) in columns {
            let checked = table.check_scaled(row, column, got[i], scale);
            failures.extend(checked.err().map(|e| format!("{name} row {row} {column}: {e:?}")));
        }
        let total = table.check_scaled(row, "eta", stages.total(), scale);
        failures.extend(total.err().map(|e| format!("{name} row {row} eta: {e:?}")));
        checked.insert(name.to_string());
    });
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!((checked.len(), later.len()), (25, 9), "{checked:?} {later:?}");
}

/// PLAN.md M8.5: the arbiter rows of M8.3 (`CoolProp-Tests.cpp`'s paper and IAPWS viscosity rows, Helium's excepted)
/// within their printed digits, for every fluid whose viscosity this step evaluates; the friction-theory fluids with
/// paper rows (SF₆, H₂S) among them. The others say the step that evaluates them. A row beyond its printed digits passes
/// only within the `Measured` bound a register entry records for its table (DIV-0021, DIV-0022; VERIFICATION.md §6),
/// and those rows are pinned. The nine rows outside the EOS's range (CO₂ at 100 and 10 000 K, Methanol at 630 K, the
/// xylenes above p_max) are evaluated with `DomainPolicy::Extrapolate`; their hardcoded models land at M8.9.
#[test]
fn viscosity_paper_rows() {
    let registry = Registry::embedded().unwrap();
    let (mut checked, mut later, mut extrapolated, mut beyond, mut failures) =
        (BTreeMap::new(), 0, Vec::new(), Vec::new(), Vec::new());
    for ((path, text), _) in transport_rows::ROWS.iter().filter(|((p, _), _)| p.ends_with(".eta.csv")) {
        let table = Fixture::parse(path, text).unwrap();
        let fluid = path.split('/').nth(1).unwrap();
        for row in (0..table.rows().len()).filter(|&r| arbitrates(fluid, table.printed(r, "provenance").unwrap())) {
            let line = table.value(row, "line").unwrap();
            let (outside, outcome) = paper_row(registry.get(fluid).unwrap(), path, &table, row);
            extrapolated.extend(outside.then(|| format!("{fluid} {line}")));
            match outcome {
                Ok(None) => later += 1,
                Ok(Some(id)) => {
                    *checked.entry(fluid).or_insert(0) += 1;
                    beyond.extend(id.map(|id| (id, fluid)));
                }
                Err(e) => failures.push(format!("{path} line {line}: {e}")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!((checked.get("SulfurHexafluoride"), checked.get("HydrogenSulfide")), (Some(&6), Some(&4)));
    assert_eq!((checked.len(), checked.values().sum::<usize>(), later), (14, 53, 104), "{checked:?}");
    let outside = ["CarbonDioxide 133", "CarbonDioxide 135", "Methanol 218", "Methanol 219", "m-Xylene 252"];
    assert_eq!(
        extrapolated,
        [&outside[..], &["m-Xylene 259", "o-Xylene 265", "o-Xylene 269", "o-Xylene 272"]].concat()
    );
    let beyond = beyond.into_iter().fold(BTreeMap::new(), |mut n, key| {
        *n.entry(key).or_insert(0) += 1;
        n
    });
    let registered = [
        (("DIV-0021", "R125"), 1),
        (("DIV-0021", "SulfurHexafluoride"), 6),
        (("DIV-0022", "Ammonia"), 1),
        (("DIV-0022", "R123"), 1),
        (("DIV-0022", "n-Octane"), 1),
        (("DIV-0022", "n-Propane"), 4),
    ];
    assert_eq!(beyond.into_iter().collect::<Vec<_>>(), registered, "rows beyond their printed digits, by entry");
}

/// One arbiter row: whether it lies outside the EOS's range (evaluated only on request), and `None` when its form
/// lands later, else the register entry whose bound it needs beyond its printed digits.
fn paper_row(
    fluid: &Fluid,
    path: &str,
    table: &Fixture<'_>,
    row: usize,
) -> (bool, Result<Option<Option<&'static str>>, String>) {
    let eta = paper_row_viscosity(fluid, table, row, DomainPolicy::Enforce);
    if matches!(eta, Err(Error::Domain(_))) {
        // A row outside the EOS's range, inside the correlation's.
        return (
            true,
            paper_row_check(path, table, row, paper_row_viscosity(fluid, table, row, DomainPolicy::Extrapolate)),
        );
    }
    (false, paper_row_check(path, table, row, eta))
}

/// `eta` against its row: `None` when its form lands later, else the register entry whose bound it needs beyond its
/// printed digits.
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
    match registered_bound(path) {
        Some((id, bound)) if (eta / want - 1.0).abs() <= bound => Ok(Some(Some(id))),
        _ => Err(format!("{error:?}")),
    }
}

/// The `Measured` bound a register entry records for the table `path` (VERIFICATION.md §6.2), with the entry's id.
fn registered_bound(path: &str) -> Option<(&'static str, f64)> {
    DIVERGENCES.iter().find_map(|d| match d.table_tolerances.iter().find(|(f, _)| *f == path) {
        Some((_, Tolerance::Relative(bound))) => Some((d.id, *bound)),
        _ => None,
    })
}

/// η at a CoolProp test row's inputs (T and a molar or mass density, a quality, or a pressure) under `domain`.
fn paper_row_viscosity(fluid: &Fluid, table: &Fixture<'_>, row: usize, domain: DomainPolicy) -> Result<f64, Error> {
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
    fluid.prop(&fluid.flash(input, &options)?, Prop::Viscosity)
}
