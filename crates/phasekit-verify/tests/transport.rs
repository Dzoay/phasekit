//! Transport as the kernel loads and evaluates it (PLAN.md M8; VERIFICATION.md §2): every embedded fluid's transport
//! section decodes on its first transport call (M8.1); the forms evaluate stage by stage from M8.5.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::collections::BTreeMap;

use phasekit_core::internal::{ConductivityModel, TransportRecord, ViscosityModel};
use phasekit_core::{DataSet, Density, Error, FlashOptions, Input, LoadError, Prop, Registry, Temperature};

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
