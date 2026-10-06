//! `data/corrections.csv` → the patches `DataSet::Corrected` applies (PLAN.md M2.7; VERIFICATION.md §6.4, §7.2).
//! A row is accepted only when its v8.0.0 value equals the record's value from the JSON bit for bit, so a correction
//! can never land on the wrong field, segment or fluid (and is dropped, not re-applied, if the data moves on).

use phasekit_core::internal::{Edit, FluidRecord, Patch};

/// The file, relative to the repository root.
pub const CORRECTIONS: &str = "data/corrections.csv";

/// One parsed row: the fluid it patches and the patch.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub fluid: String,
    pub patch: Patch,
    /// The v8.0.0 value the row says it replaces.
    pub v800: f64,
}

/// The rows of the file, comment lines and the header skipped.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let mut lines = text.lines().enumerate().filter(|(_, l)| !l.starts_with('#') && !l.trim().is_empty());
    match lines.next() {
        Some((_, "div,fluid,field,v8.0.0,corrected,citation")) => {}
        other => return Err(format!("{CORRECTIONS}: header line expected, found {other:?}")),
    }
    for (i, line) in lines {
        let at = |m: String| format!("{CORRECTIONS}:{}: {m}", i + 1);
        let cells: Vec<&str> = line.splitn(6, ',').collect();
        let [div, fluid, field, v800, corrected, citation] = cells[..] else {
            return Err(at(format!("6 cells expected, found {}", cells.len())));
        };
        let number = |s: &str| s.parse::<f64>().map_err(|e| at(format!("{s:?} is not a number: {e}")));
        let (v800, value) = (number(v800)?, number(corrected)?);
        if !(div.starts_with("DIV-") && citation.trim().len() > 10) {
            return Err(at("a DIV id and a citation are required".into()));
        }
        let edit = match field.split_once('[') {
            None if field == "gas_constant" => Edit::GasConstant(value),
            None if field == "rhomolar_reducing" => Edit::ReducingDensity(value),
            None if field == "molar_mass" => Edit::MolarMass(value),
            Some(("melting_p0", k)) => {
                let segment = k.trim_end_matches(']').parse().map_err(|_| at(format!("bad segment in {field}")))?;
                Edit::MeltingP0 { segment, p0: value }
            }
            _ => return Err(at(format!("unknown field {field}"))),
        };
        rows.push(Row { fluid: fluid.into(), patch: Patch { divergence: div.into(), edit }, v800 });
    }
    Ok(rows)
}

/// The value `edit` replaces in `record`, or `None` when the record has no such field (a melting segment it lacks).
fn current(record: &FluidRecord, edit: &Edit) -> Option<f64> {
    Some(match *edit {
        Edit::GasConstant(_) => record.eos.gas_constant,
        Edit::ReducingDensity(_) => record.eos.rho_reducing,
        Edit::MolarMass(_) => record.molar_mass,
        Edit::MeltingP0 { segment, .. } => record.melting.get(usize::from(segment))?.p0,
        _ => return None,
    })
}

/// Attaches each row's patch to its fluid's record after checking the v8.0.0 value bit for bit; every row must find
/// its fluid, and no fluid gets two edits of one field.
pub fn attach(rows: &[Row], records: &mut [FluidRecord]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for row in rows {
        let Some(record) = records.iter_mut().find(|r| r.name == row.fluid) else {
            errors.push(format!("{}: unknown fluid {}", row.patch.divergence, row.fluid));
            continue;
        };
        let same_field = |p: &Patch| core::mem::discriminant(&p.edit) == core::mem::discriminant(&row.patch.edit);
        match current(record, &row.patch.edit) {
            None => {
                errors.push(format!("{}: {} has no field for {:?}", row.patch.divergence, row.fluid, row.patch.edit))
            }
            Some(v) if v.to_bits() != row.v800.to_bits() => errors.push(format!(
                "{}: {} {:?} replaces {}, but the v8.0.0 value is {v}",
                row.patch.divergence, row.fluid, row.patch.edit, row.v800
            )),
            Some(_) if record.corrections.iter().any(same_field) => {
                errors.push(format!("{}: {} already has an edit of this field", row.patch.divergence, row.fluid));
            }
            Some(_) => record.corrections.push(row.patch.clone()),
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repo;

    fn records() -> Vec<FluidRecord> {
        let sources = super::super::load(&Repo::locate()).unwrap();
        sources.iter().map(|s| super::super::record::to_record(s).unwrap()).collect()
    }

    const HEADER: &str = "div,fluid,field,v8.0.0,corrected,citation\n";

    /// PLAN.md M2.7 (VERIFICATION.md §7.2): the three shipped rows attach to R1234ze(E), Water and Nitrogen; a row
    /// whose v8.0.0 value is not the JSON's is refused (the seed's DIV-0002 named ice V's segment 2, 350.1 MPa, not ice
    /// VI's segment 3, 623.4 MPa), and so are an unknown fluid, field or segment, a second edit of one field, a row
    /// without a citation and a file without its header.
    #[test]
    fn corrections_check_the_value_they_replace() {
        let shipped = parse(&Repo::locate().read(CORRECTIONS).unwrap()).unwrap();
        let base = records();
        let mut all = base.clone();
        attach(&shipped, &mut all).unwrap();
        let patched: Vec<(&str, usize)> =
            all.iter().filter(|r| !r.corrections.is_empty()).map(|r| (r.name.as_str(), r.corrections.len())).collect();
        assert_eq!(patched, [("Nitrogen", 1), ("R1234ze(E)", 1), ("Water", 1)]);

        let cite = "a citation with, commas";
        let refused = |row: &str| {
            let rows = parse(&format!("{HEADER}{row},{cite}\n")).unwrap();
            attach(&rows, &mut base.clone()).unwrap_err().join("\n")
        };
        let segment_2 = refused("DIV-0002,Water,melting_p0[2],623400000.0,632400000.0");
        assert!(segment_2.contains("replaces 623400000, but the v8.0.0 value is 350100000"), "{segment_2}");
        assert!(refused("DIV-0001,R1234ze(E),gas_constant,8.3144621,8.314472").contains("value is 8.314472"));
        assert!(refused("DIV-0003,Nitrogenn,rhomolar_reducing,1,2").contains("unknown fluid Nitrogenn"));
        assert!(refused("DIV-0002,Water,melting_p0[4],1,2").contains("Water has no field for"));
        assert!(refused("DIV-0002,Acetone,melting_p0[0],1,2").contains("Acetone has no field for"));
        let twice = format!(
            "{HEADER}DIV-0001,Air,molar_mass,0.02896546,0.029,{cite}\nDIV-0001,Air,molar_mass,0.02896546,0.03,{cite}\n"
        );
        let err = attach(&parse(&twice).unwrap(), &mut base.clone()).unwrap_err();
        assert_eq!(err, ["DIV-0001: Air already has an edit of this field"]);
        assert!(
            parse(&format!("{HEADER}DIV-0001,Air,density,1,2,{cite}\n")).unwrap_err().contains("unknown field density")
        );
        assert!(parse(&format!("{HEADER}DIV-0001,Air,molar_mass,1,2,x\n")).unwrap_err().contains("a citation"));
        assert!(
            parse(&format!("{HEADER}DIV-0001,Air,molar_mass,1,2,0123456789\n")).is_err(),
            "10 characters are too few"
        );
        assert!(parse(&format!("{HEADER}DIV-0001,Air,molar_mass,1,2,01234567890\n")).is_ok());
        assert!(parse(&format!("{HEADER}XIV-0001,Air,molar_mass,1,2,{cite}\n")).is_err(), "not a DIV id");
        assert!(parse(&format!("{HEADER}DIV-0001,Air,molar_mass,one,2,{cite}\n")).unwrap_err().contains("\"one\""));
        assert!(parse(&format!("{HEADER}DIV-0001,Air\n")).unwrap_err().contains("6 cells expected, found 2"));
        assert!(parse("div,fluid\n").unwrap_err().contains("header line expected"));
        let rows =
            parse(&format!("# c\n{HEADER}DIV-0002,Water,melting_p0[3],623400000.0,632400000.0,{cite}\n")).unwrap();
        assert_eq!(rows[0].patch.edit, Edit::MeltingP0 { segment: 3, p0: 632.4e6 });
    }
}
