//! Citations (PLAN.md M2.10; ROT-142..144): every default model's sources, checked against CoolProp's bibliography
//! and the reviewed `data/citations.csv`, and split into role-tagged citations (map 13 R5).
//!
//! - Every key of `BibTeX_EOS` and `BibTeX_CP0` must resolve in `CoolPropBibTeXLibrary.bib` (map 13 R6: CoolProp's
//!   docs generator writes an empty citation instead).
//! - The coefficients' work needs a DOI or report id, from the `.bib` or the reviewed file, or a written waiver (map 13
//!   R2: 19 EOS keys have no DOI in the `.bib`).
//! - A model the reviewed file marks unpublished gets `DataTerms::Unpublished` (map 13 R7): only oracle rows check it.

use phasekit_core::internal::FluidRecord;
use phasekit_core::{Citation, CitationRole, DataTerms};

/// The reviewed file, relative to the repository root.
pub const CITATIONS: &str = "data/citations.csv";
/// CoolProp's bibliography in the pinned checkout.
pub const BIB: &str = "reference/CoolProp/CoolPropBibTeXLibrary.bib";

/// The `.bib` as (key, DOI) per entry.
pub type Bib = Vec<(String, Option<String>)>;

/// The `.bib`: (key, DOI) per entry.
pub fn parse_bib(text: &str) -> Bib {
    let mut entries = Vec::new();
    for chunk in text.split("\n@").skip(1) {
        let Some((_, rest)) = chunk.split_once('{') else { continue };
        let Some((key, body)) = rest.split_once(',') else { continue };
        let doi = body.lines().find_map(|line| {
            let (field, value) = line.split_once('=')?;
            (field.trim().eq_ignore_ascii_case("doi"))
                .then(|| value.trim().trim_end_matches(',').trim_matches(['{', '}']).to_string())
        });
        entries.push((key.trim().to_string(), doi.filter(|d| !d.is_empty())));
    }
    if let Some(first) = text.strip_prefix('@').and_then(|t| t.split_once('{')).and_then(|(_, r)| r.split_once(',')) {
        entries.insert(0, (first.0.trim().to_string(), None));
    }
    entries
}

/// One reviewed row.
#[derive(Clone, Debug, PartialEq)]
pub struct Reviewed {
    pub key: String,
    pub doi: Option<String>,
    pub role: Option<CitationRole>,
    pub unpublished: bool,
    pub note: String,
}

/// The reviewed rows.
pub fn parse(text: &str) -> Result<Vec<Reviewed>, String> {
    let mut rows = Vec::new();
    let mut lines = text.lines().enumerate().filter(|(_, l)| !l.starts_with('#') && !l.trim().is_empty());
    match lines.next() {
        Some((_, "key,doi,role,unpublished,note")) => {}
        other => return Err(format!("{CITATIONS}: header line expected, found {other:?}")),
    }
    for (i, line) in lines {
        let at = |m: String| format!("{CITATIONS}:{}: {m}", i + 1);
        let cells: Vec<&str> = line.splitn(5, ',').collect();
        let [key, doi, role, unpublished, note] = cells[..] else {
            return Err(at(format!("5 cells expected, found {}", cells.len())));
        };
        let role = match role {
            "" => None,
            "Coefficients" => Some(CitationRole::Coefficients),
            "IdealGas" => Some(CitationRole::IdealGas),
            "Erratum" => Some(CitationRole::Erratum),
            "Related" => Some(CitationRole::Related),
            other => return Err(at(format!("unknown role {other}"))),
        };
        let unpublished = match unpublished {
            "" => false,
            "unpublished" => true,
            other => return Err(at(format!("unpublished column holds {other:?}"))),
        };
        if doi.is_empty() && !note.starts_with("waived: ") && role.is_none() {
            return Err(at(format!("{key}: neither an identifier nor a \"waived: \" note")));
        }
        let doi = Some(doi.to_string()).filter(|d| !d.is_empty());
        rows.push(Reviewed { key: key.into(), doi, role, unpublished, note: note.into() });
    }
    Ok(rows)
}

/// The citations of one model from its `BibTeX_EOS` (comma-joined, coefficients first) and `BibTeX_CP0` strings.
pub fn citations(
    eos: &str,
    cp0: &str,
    bib: &[(String, Option<String>)],
    reviewed: &[Reviewed],
) -> Result<(Vec<Citation>, bool), String> {
    let lookup = |key: &str| -> Result<(Option<String>, Option<&Reviewed>), String> {
        let (_, doi) = bib.iter().find(|(k, _)| k == key).ok_or(format!("{key} is not in {BIB}"))?;
        Ok((doi.clone(), reviewed.iter().find(|r| r.key == key)))
    };
    let mut out: Vec<Citation> = Vec::new();
    let mut unpublished = false;
    let keys: Vec<&str> = eos.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
    for (i, key) in keys.iter().enumerate() {
        let (doi, row) = lookup(key)?;
        let role =
            if i == 0 { CitationRole::Coefficients } else { row.and_then(|r| r.role).unwrap_or(CitationRole::Related) };
        if i == 0 {
            unpublished = row.is_some_and(|r| r.unpublished);
            let waived = row.is_some_and(|r| r.note.starts_with("waived: "));
            if doi.is_none() && row.and_then(|r| r.doi.as_ref()).is_none() && !waived {
                return Err(format!("{key}: no DOI or report id, and no waiver in {CITATIONS}"));
            }
        }
        let doi = doi.or_else(|| row.and_then(|r| r.doi.clone()));
        out.push(Citation { key: (*key).into(), doi: doi.map(Into::into), role });
    }
    let cp0 = cp0.trim();
    if !cp0.is_empty() && !keys.contains(&cp0) {
        let (doi, row) = lookup(cp0)?;
        let doi = doi.or_else(|| row.and_then(|r| r.doi.clone()));
        out.push(Citation { key: cp0.into(), doi: doi.map(Into::into), role: CitationRole::IdealGas });
    }
    if out.is_empty() {
        return Err("no BibTeX_EOS key".into());
    }
    Ok((out, unpublished))
}

/// Checks the reviewed file against the `.bib` (every key exists; a DOI is added only where the `.bib` has none) and
/// gives every record its role-tagged citations, primary DOI and terms.
pub fn attach(
    sources: &[super::Source],
    records: &mut [FluidRecord],
    bib: &[(String, Option<String>)],
    reviewed: &[Reviewed],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for row in reviewed {
        match bib.iter().find(|(k, _)| *k == row.key) {
            None => errors.push(format!("{CITATIONS}: {} is not in {BIB}", row.key)),
            Some((_, Some(doi))) if row.doi.is_some() => {
                errors.push(format!("{CITATIONS}: {} already has DOI {doi} in the .bib", row.key));
            }
            Some(_) => {}
        }
    }
    for (source, record) in sources.iter().zip(records.iter_mut()) {
        let eos = &source.fluid.eos[0];
        match citations(&eos.bibtex_eos, &eos.bibtex_cp0, bib, reviewed) {
            Ok((list, unpublished)) => {
                record.source.bibkey = list[0].key.clone();
                record.source.doi = list[0].doi.clone();
                record.source.citations = list;
                if unpublished {
                    record.source.terms = DataTerms::Unpublished;
                }
            }
            Err(e) => errors.push(format!("{}: {e}", source.file)),
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repo;

    fn inputs() -> (Vec<super::super::Source>, Bib, Vec<Reviewed>) {
        let repo = Repo::locate();
        let sources = super::super::load(&repo).unwrap();
        (sources, parse_bib(&repo.read(BIB).unwrap()), parse(&repo.read(CITATIONS).unwrap()).unwrap())
    }

    /// The shipped records, decoded from datagen's blobs.
    fn records() -> Vec<FluidRecord> {
        let (_, entries) = super::super::generate(&Repo::locate()).unwrap();
        entries.iter().map(|e| FluidRecord::decode(&e.blob).unwrap()).collect()
    }

    /// Rot: ROT-143 (map 13 R6). Every key of every default model's `BibTeX_EOS` and `BibTeX_CP0` resolves in CoolProp's
    /// bibliography (298 entries); a key that does not is an error, and so is a reviewed row for an unknown key or one
    /// that adds a DOI the `.bib` already has.
    #[test]
    fn every_bibkey_resolves() {
        let (sources, bib, reviewed) = inputs();
        assert!(bib.len() >= 298, "{}", bib.len());
        let mut keys = 0;
        for s in &sources {
            let eos = &s.fluid.eos[0];
            let (list, _) = citations(&eos.bibtex_eos, &eos.bibtex_cp0, &bib, &reviewed).unwrap();
            keys += list.len();
        }
        assert!(keys > 136, "{keys}");
        assert_eq!(
            citations("Span-JPCRD-2001", "", &bib, &reviewed).unwrap_err(),
            format!("Span-JPCRD-2001 is not in {BIB}")
        );
        assert!(citations("Span-JPCRD-2000", "Nope-1999", &bib, &reviewed).unwrap_err().contains("Nope-1999"));
        let mut records: Vec<FluidRecord> =
            sources.iter().map(|s| super::super::record::to_record(s).unwrap()).collect();
        let mut bad = reviewed.clone();
        bad.push(Reviewed {
            key: "Nope-1999".into(),
            doi: None,
            role: None,
            unpublished: false,
            note: "waived: x".into(),
        });
        bad.push(Reviewed {
            key: "Span-JPCRD-2000".into(),
            doi: Some("10.1/x".into()),
            role: None,
            unpublished: false,
            note: "x".into(),
        });
        let errors = attach(&sources, &mut records, &bib, &bad).unwrap_err();
        assert_eq!(
            errors,
            [
                format!("{CITATIONS}: Nope-1999 is not in {BIB}"),
                format!("{CITATIONS}: Span-JPCRD-2000 already has DOI 10.1063/1.1349047 in the .bib"),
            ]
        );
    }

    /// Rot: ROT-143 (map 13 R2). Every default model's coefficients have a DOI or report id, from the `.bib` or the
    /// reviewed file (7 of the 19 keys without one there), or a written waiver (books, a thesis, proceedings, the
    /// unpublished models); a key with neither is refused.
    #[test]
    fn every_default_source_has_an_identifier() {
        let (_, _, reviewed) = inputs();
        let records = records();
        let mut waived = Vec::new();
        for r in &records {
            let primary = &r.source.citations[0];
            assert_eq!(primary.role, CitationRole::Coefficients, "{}", r.name);
            assert_eq!((&*r.source.bibkey, &r.source.doi), (&*primary.key, &primary.doi), "{}", r.name);
            if primary.doi.is_none() {
                let row = reviewed.iter().find(|w| *w.key == *primary.key).unwrap();
                assert!(row.note.starts_with("waived: "), "{}", r.name);
                waived.push(r.name.as_str());
            }
        }
        let want = ["Benzene", "CycloPropane", "D4", "Dichloroethane", "Fluorine", "Methanol", "Neon", "Propylene"]
            .into_iter()
            .chain(["Propyne", "R113", "R12", "R124", "SES36", "n-Hexane", "n-Octane", "n-Pentane"]);
        assert_eq!(waived, want.collect::<Vec<_>>());
        let ammonia = records.iter().find(|r| r.name == "Ammonia").unwrap();
        assert_eq!(ammonia.source.doi.as_deref(), Some("10.1063/5.0128269"));
        let helium = records.iter().find(|r| r.name == "Helium").unwrap();
        assert_eq!(helium.source.doi.as_deref(), Some("NIST IR 8474"));
        let bib = vec![("X-2020".to_string(), None)];
        assert!(citations("X-2020", "", &bib, &[]).unwrap_err().contains("no DOI or report id, and no waiver"));
        assert!(
            parse("key,doi,role,unpublished,note\nX-2020,,,,a note\n").unwrap_err().contains("neither an identifier")
        );
    }

    /// Rot: ROT-144 (map 13 R5). CoolProp's composite citation strings split into role-tagged citations: the first
    /// key holds the coefficients, an erratum is an erratum, the rest are related works until reviewed; a separate
    /// c_p⁰ source is the ideal-gas citation.
    #[test]
    fn composite_citations_are_split_by_role() {
        let records = records();
        let roles = |name: &str| -> Vec<(String, CitationRole)> {
            let r = records.iter().find(|r| r.name == name).unwrap();
            r.source.citations.iter().map(|c| (c.key.to_string(), c.role)).collect()
        };
        use CitationRole::{Coefficients, Erratum, IdealGas, Related};
        assert_eq!(
            roles("EthyleneOxide"),
            [
                ("Thol-CES-2015".into(), Coefficients),
                ("Thol-CES-2015-CORR".into(), Erratum),
                ("Thol-THESIS-2015".into(), Related),
            ]
        );
        assert_eq!(
            roles("Oxygen"),
            [("Schmidt-FPE-1985".into(), Coefficients), ("Stewart-JPCRD-1991".into(), Related)]
        );
        assert_eq!(roles("MM"), [("Thol-FPE-2016-MM".into(), Coefficients), ("Thol-THESIS-2015".into(), Related)]);
        let with_cp0: Vec<&str> = records
            .iter()
            .filter(|r| r.source.citations.iter().any(|c| c.role == IdealGas))
            .map(|r| r.name.as_str())
            .collect();
        assert!(with_cp0.contains(&"R123") || !with_cp0.is_empty(), "{with_cp0:?}");
        assert!(parse("key,doi,role,unpublished,note\nX,,Primary,,x\n").unwrap_err().contains("unknown role Primary"));
    }

    /// Rot: ROT-142 (map 13 R7). Propylene, SES36 and Neon have no published paper: their sources are `Unpublished`,
    /// and no arbiter record gives them a paper table (Neon's record says so: `Unpublished`, no tables), so only oracle
    /// rows can check them.
    #[test]
    fn unpublished_models_have_no_paper_arbiter() {
        let records = records();
        let unpublished: Vec<&str> =
            records.iter().filter(|r| r.source.terms == DataTerms::Unpublished).map(|r| r.name.as_str()).collect();
        assert_eq!(unpublished, ["Neon", "Propylene", "SES36"]);
        for name in unpublished {
            let records = phasekit_verify::ARBITERS.iter().filter(|a| a.fluid == name);
            let unpublished_only = |a: &&phasekit_verify::Arbiter| {
                a.tables.is_empty() && a.status == phasekit_verify::ArbiterStatus::Unpublished
            };
            assert!(
                records.clone().all(|a| unpublished_only(&a)),
                "{name}: a paper table arbitrates an unpublished model"
            );
        }
        assert!(parse("key,doi,role,unpublished,note\nX,,,maybe,waived: x\n").unwrap_err().contains("\"maybe\""));
    }
}
