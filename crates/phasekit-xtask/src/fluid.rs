//! `cargo xtask fluid list|show|diff` (PLAN.md M2.9a; user decision FD1): the shipped fluid data in readable form.
//!
//! Each committed blob decodes to its record, and the record prints as JSON: deterministic key order, every float in
//! its shortest round-trip form, term kinds tagged. The form is complete: parsing it and re-encoding reproduces the
//! blob byte for byte, so reviewing the dump is reviewing what ships. `diff` compares Parity with Corrected, or this
//! tree's blobs with those of another git revision (`--base <rev>`; CI writes it to the job summary on data PRs).
//!
//! - `cargo xtask fluid list`: canonical names, aliases and blob sizes.
//! - `cargo xtask fluid show <name> [--corrected]`: one record (Parity unless `--corrected`).
//! - `cargo xtask fluid diff [--base <rev>]`: every changed field, `<fluid>: <path>: <old> -> <new>`.

use std::process::ExitCode;

use phasekit_core::internal::{
    DoubleExponentialTerm, Edit, EosRecord, FluidRecord, GaoBTerm, GaussianTerm, IdealTerm, Lemmon2005Term,
    MeltingSegment, NonAnalyticTerm, OffsetReference, Patch, PowerTerm, SaStamp,
};
use phasekit_core::{CriticalOrigin, CriticalPoint, DataSet, DataTerms, Limits, Source};
use serde_json::{Map, Value, json};

use crate::repo::Repo;

const BLOBS: &str = "crates/phasekit-data/blobs";

fn num(x: f64) -> Value {
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

fn opt_str(s: Option<&str>) -> Value {
    s.map_or(Value::Null, |s| Value::String(s.into()))
}

/// The record as JSON.
pub fn to_json(r: &FluidRecord) -> Value {
    let e = &r.eos;
    let l = &r.limits;
    let terms = |label: &str, rows: Vec<Value>| (label.to_string(), Value::Array(rows));
    let mut eos = Map::new();
    for (k, v) in [("gas_constant", e.gas_constant), ("t_reducing", e.t_reducing), ("rho_reducing", e.rho_reducing)]
        .into_iter()
        .chain([("rho_max", e.rho_max)])
    {
        eos.insert(k.into(), num(v));
    }
    let lists = [
        terms("power", e.power.iter().map(|p| json!({"n": num(p.n), "t": num(p.t), "d": p.d, "l": p.l, "c": num(p.c)})).collect()),
        terms(
            "lemmon2005",
            e.lemmon2005.iter().map(|p| json!({"n": num(p.n), "t": num(p.t), "d": p.d, "l": p.l, "m": num(p.m)})).collect(),
        ),
        terms(
            "double_exponential",
            e.double_exponential
                .iter()
                .map(|p| json!({"n": num(p.n), "t": num(p.t), "d": p.d, "gd": num(p.gd), "ld": p.ld, "gt": num(p.gt), "lt": num(p.lt)}))
                .collect(),
        ),
        terms(
            "gaussian",
            e.gaussian
                .iter()
                .map(|p| json!({"n": num(p.n), "t": num(p.t), "d": p.d, "eta": num(p.eta), "epsilon": num(p.epsilon), "beta": num(p.beta), "gamma": num(p.gamma)}))
                .collect(),
        ),
        terms(
            "gao_b",
            e.gao_b
                .iter()
                .map(|p| json!({"n": num(p.n), "t": num(p.t), "d": p.d, "eta": num(p.eta), "epsilon": num(p.epsilon), "beta": num(p.beta), "gamma": num(p.gamma), "b": num(p.b)}))
                .collect(),
        ),
        terms(
            "non_analytic",
            e.non_analytic
                .iter()
                .map(|p| json!({"n": num(p.n), "a": num(p.a), "b": num(p.b), "beta": num(p.beta), "A": num(p.big_a), "B": num(p.big_b), "C": num(p.big_c), "D": num(p.big_d)}))
                .collect(),
        ),
        terms("ideal", e.ideal.iter().map(ideal_json).collect()),
    ];
    eos.extend(lists);
    json!({
        "name": r.name,
        "aliases": r.aliases,
        "cas": opt_str(r.cas.as_deref()),
        "refprop_name": opt_str(r.refprop_name.as_deref()),
        "inchi_key": opt_str(r.inchi_key.as_deref()),
        "molar_mass": num(r.molar_mass),
        "source": {"bibkey": &*r.source.bibkey, "doi": opt_str(r.source.doi.as_deref()), "terms": terms_name(r.source.terms)},
        "limits": {"t_min": num(l.t_min()), "t_max": num(l.t_max()), "p_max": num(l.p_max()), "t_triple": l.t_triple().map_or(Value::Null, num)},
        "critical": r.critical.map_or(Value::Null, |c| json!({"t": num(c.t), "p": num(c.p), "rho": num(c.rho), "origin": if c.origin == CriticalOrigin::Model { "Model" } else { "Published" }})),
        "eos": Value::Object(eos),
        "superancillary_fit": r.superancillary_fit.map_or(Value::Null, |s| json!({"shape": format!("{:016x}", s.shape.get()), "gas_constant": num(s.gas_constant), "rho_reducing": num(s.rho_reducing)})),
        "melting": r.melting.iter().map(|s| json!({"t0": num(s.t0), "p0": num(s.p0), "t_min": num(s.t_min), "t_max": num(s.t_max)})).collect::<Vec<_>>(),
        "corrections": r.corrections.iter().map(patch_json).collect::<Vec<_>>(),
    })
}

fn terms_name(t: DataTerms) -> &'static str {
    match t {
        DataTerms::Unpublished => "Unpublished",
        DataTerms::Restricted => "Restricted",
        _ => "Published",
    }
}

fn ideal_json(t: &IdealTerm) -> Value {
    match *t {
        IdealTerm::Lead { a1, a2 } => json!({"kind": "Lead", "a1": num(a1), "a2": num(a2)}),
        IdealTerm::LogTau { a } => json!({"kind": "LogTau", "a": num(a)}),
        IdealTerm::Power { n, t } => json!({"kind": "Power", "n": num(n), "t": num(t)}),
        IdealTerm::PlanckEinstein { n, theta } => json!({"kind": "PlanckEinstein", "n": num(n), "theta": num(theta)}),
        IdealTerm::PlanckEinsteinGeneralized { n, theta, c, d } => {
            json!({"kind": "PlanckEinsteinGeneralized", "n": num(n), "theta": num(theta), "c": num(c), "d": num(d)})
        }
        IdealTerm::Cp0Power { c, t, tc, t0 } => {
            json!({"kind": "Cp0Power", "c": num(c), "t": num(t), "tc": num(tc), "t0": num(t0)})
        }
        IdealTerm::Offset { a1, a2, reference } => {
            let reference = match reference {
                OffsetReference::Nbp => "NBP",
                OffsetReference::Other => "OTH",
                OffsetReference::Custom => "CUSTOM",
                _ => "IIR",
            };
            json!({"kind": "Offset", "a1": num(a1), "a2": num(a2), "reference": reference})
        }
        _ => json!({"kind": "unknown"}),
    }
}

fn patch_json(p: &Patch) -> Value {
    let edit = match p.edit {
        Edit::GasConstant(r) => json!({"gas_constant": num(r)}),
        Edit::ReducingDensity(rho) => json!({"rho_reducing": num(rho)}),
        Edit::MolarMass(m) => json!({"molar_mass": num(m)}),
        Edit::MeltingP0 { segment, p0 } => json!({"melting_p0": {"segment": segment, "p0": num(p0)}}),
        _ => json!({"unknown": null}),
    };
    json!({"divergence": &*p.divergence, "edit": edit})
}

/// Reads the dump back.
pub fn from_json(v: &Value) -> Result<FluidRecord, String> {
    let f = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).ok_or(format!("{k}: not a number"));
    let u = |v: &Value, k: &str| {
        v.get(k).and_then(Value::as_u64).and_then(|x| u8::try_from(x).ok()).ok_or(format!("{k}: not a small integer"))
    };
    let s =
        |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string).ok_or(format!("{k}: not a string"));
    let os = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let list = |v: &Value, k: &str| v.get(k).and_then(Value::as_array).cloned().ok_or(format!("{k}: not a list"));

    let e = v.get("eos").ok_or("no eos")?;
    let mut eos = EosRecord::new(f(e, "gas_constant")?, f(e, "t_reducing")?, f(e, "rho_reducing")?, f(e, "rho_max")?);
    for p in list(e, "power")? {
        eos.power.push(PowerTerm::new(f(&p, "n")?, f(&p, "t")?, u(&p, "d")?, u(&p, "l")?, f(&p, "c")?));
    }
    for p in list(e, "lemmon2005")? {
        eos.lemmon2005.push(Lemmon2005Term {
            n: f(&p, "n")?,
            t: f(&p, "t")?,
            d: u(&p, "d")?,
            l: u(&p, "l")?,
            m: f(&p, "m")?,
        });
    }
    for p in list(e, "double_exponential")? {
        let (n, t, d, gd, ld, gt, lt) =
            (f(&p, "n")?, f(&p, "t")?, u(&p, "d")?, f(&p, "gd")?, u(&p, "ld")?, f(&p, "gt")?, f(&p, "lt")?);
        eos.double_exponential.push(DoubleExponentialTerm { n, t, d, gd, ld, gt, lt });
    }
    for p in list(e, "gaussian")? {
        let (n, t, d) = (f(&p, "n")?, f(&p, "t")?, u(&p, "d")?);
        let (eta, epsilon, beta, gamma) = (f(&p, "eta")?, f(&p, "epsilon")?, f(&p, "beta")?, f(&p, "gamma")?);
        eos.gaussian.push(GaussianTerm { n, t, d, eta, epsilon, beta, gamma });
    }
    for p in list(e, "gao_b")? {
        let (n, t, d, b) = (f(&p, "n")?, f(&p, "t")?, u(&p, "d")?, f(&p, "b")?);
        let (eta, epsilon, beta, gamma) = (f(&p, "eta")?, f(&p, "epsilon")?, f(&p, "beta")?, f(&p, "gamma")?);
        eos.gao_b.push(GaoBTerm { n, t, d, eta, epsilon, beta, gamma, b });
    }
    for p in list(e, "non_analytic")? {
        let (n, a, b, beta) = (f(&p, "n")?, f(&p, "a")?, f(&p, "b")?, f(&p, "beta")?);
        let (big_a, big_b, big_c, big_d) = (f(&p, "A")?, f(&p, "B")?, f(&p, "C")?, f(&p, "D")?);
        eos.non_analytic.push(NonAnalyticTerm { n, a, b, beta, big_a, big_b, big_c, big_d });
    }
    for t in list(e, "ideal")? {
        eos.ideal.push(match s(&t, "kind")?.as_str() {
            "Lead" => IdealTerm::Lead { a1: f(&t, "a1")?, a2: f(&t, "a2")? },
            "LogTau" => IdealTerm::LogTau { a: f(&t, "a")? },
            "Power" => IdealTerm::Power { n: f(&t, "n")?, t: f(&t, "t")? },
            "PlanckEinstein" => IdealTerm::PlanckEinstein { n: f(&t, "n")?, theta: f(&t, "theta")? },
            "PlanckEinsteinGeneralized" => IdealTerm::PlanckEinsteinGeneralized {
                n: f(&t, "n")?,
                theta: f(&t, "theta")?,
                c: f(&t, "c")?,
                d: f(&t, "d")?,
            },
            "Cp0Power" => IdealTerm::Cp0Power { c: f(&t, "c")?, t: f(&t, "t")?, tc: f(&t, "tc")?, t0: f(&t, "t0")? },
            "Offset" => {
                let reference = match s(&t, "reference")?.as_str() {
                    "IIR" => OffsetReference::Iir,
                    "NBP" => OffsetReference::Nbp,
                    "OTH" => OffsetReference::Other,
                    "CUSTOM" => OffsetReference::Custom,
                    other => return Err(format!("unknown offset reference {other}")),
                };
                IdealTerm::Offset { a1: f(&t, "a1")?, a2: f(&t, "a2")?, reference }
            }
            other => return Err(format!("unknown ideal-gas kind {other}")),
        });
    }

    let src = v.get("source").ok_or("no source")?;
    let terms = match s(src, "terms")?.as_str() {
        "Published" => DataTerms::Published,
        "Unpublished" => DataTerms::Unpublished,
        "Restricted" => DataTerms::Restricted,
        other => return Err(format!("unknown data terms {other}")),
    };
    let source = Source { bibkey: s(src, "bibkey")?.into(), doi: os(src, "doi").map(Into::into), terms };
    let lim = v.get("limits").ok_or("no limits")?;
    let limits = Limits::new(f(lim, "t_min")?, f(lim, "t_max")?, f(lim, "p_max")?).map_err(|e| e.to_string())?;
    let limits = match lim.get("t_triple").and_then(Value::as_f64) {
        Some(t) => limits.with_t_triple(t),
        None => limits,
    };
    let mut r = FluidRecord::new(&s(v, "name")?, f(v, "molar_mass")?, source, eos, limits);
    r.aliases = list(v, "aliases")?
        .iter()
        .map(|a| a.as_str().map(str::to_string).ok_or("alias: not a string"))
        .collect::<Result<_, _>>()?;
    (r.cas, r.refprop_name, r.inchi_key) = (os(v, "cas"), os(v, "refprop_name"), os(v, "inchi_key"));
    if let Some(c) = v.get("critical").filter(|c| !c.is_null()) {
        let origin = if s(c, "origin")? == "Model" { CriticalOrigin::Model } else { CriticalOrigin::Published };
        r.critical = Some(CriticalPoint { t: f(c, "t")?, p: f(c, "p")?, rho: f(c, "rho")?, origin });
    }
    if let Some(fit) = v.get("superancillary_fit").filter(|x| !x.is_null()) {
        let shape = u64::from_str_radix(&s(fit, "shape")?, 16).map_err(|e| format!("shape: {e}"))?;
        let shape = phasekit_core::internal::model_key(shape);
        r.superancillary_fit =
            Some(SaStamp { shape, gas_constant: f(fit, "gas_constant")?, rho_reducing: f(fit, "rho_reducing")? });
    }
    for m in list(v, "melting")? {
        r.melting.push(MeltingSegment {
            t0: f(&m, "t0")?,
            p0: f(&m, "p0")?,
            t_min: f(&m, "t_min")?,
            t_max: f(&m, "t_max")?,
        });
    }
    for p in list(v, "corrections")? {
        let edit = p.get("edit").ok_or("correction without an edit")?;
        let edit = if let Some(x) = edit.get("gas_constant").and_then(Value::as_f64) {
            Edit::GasConstant(x)
        } else if let Some(x) = edit.get("rho_reducing").and_then(Value::as_f64) {
            Edit::ReducingDensity(x)
        } else if let Some(x) = edit.get("molar_mass").and_then(Value::as_f64) {
            Edit::MolarMass(x)
        } else if let Some(m) = edit.get("melting_p0") {
            Edit::MeltingP0 { segment: u(m, "segment")?, p0: f(m, "p0")? }
        } else {
            return Err(format!("unknown edit {edit}"));
        };
        r.corrections.push(Patch { divergence: s(&p, "divergence")?.into(), edit });
    }
    Ok(r)
}

/// The JSON text of a record: pretty, keys sorted (serde_json's map), floats in shortest round-trip form.
pub fn dump(r: &FluidRecord) -> String {
    serde_json::to_string_pretty(&to_json(r)).unwrap_or_default()
}

/// Every difference between two JSON values: `<path>: <old> -> <new>`.
pub fn differences(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let (va, vb) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                differences(&format!("{path}.{k}"), va, vb, out);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (va, vb)) in x.iter().zip(y).enumerate() {
                differences(&format!("{path}[{i}]"), va, vb, out);
            }
        }
        _ if a != b => out.push(format!("{}: {a} -> {b}", path.trim_start_matches('.'))),
        _ => {}
    }
}

/// Field-by-field differences of two record sets, by fluid name (a fluid in one set only is reported too).
pub fn diff(old: &[FluidRecord], new: &[FluidRecord]) -> Vec<String> {
    let mut names: Vec<&str> = old.iter().chain(new).map(|r| r.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    let mut out = Vec::new();
    for name in names {
        let find = |set: &[FluidRecord]| set.iter().find(|r| r.name == name).map_or(Value::Null, to_json);
        let mut lines = Vec::new();
        differences("", &find(old), &find(new), &mut lines);
        out.extend(lines.into_iter().map(|l| format!("{name}: {l}")));
    }
    out
}

/// The records of the blobs in `files` (name, bytes), sorted by name.
fn decode_all(files: Vec<(String, Vec<u8>)>) -> Result<Vec<FluidRecord>, String> {
    let mut records = Vec::new();
    for (name, bytes) in files {
        records.push(FluidRecord::decode(&bytes).map_err(|e| format!("{name}: {e}"))?);
    }
    records.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(records)
}

/// The committed blobs of this tree, decoded.
pub fn records(repo: &Repo) -> Result<Vec<FluidRecord>, String> {
    let names = repo.file_names(BLOBS)?;
    let files = names
        .into_iter()
        .map(|n| repo.read_bytes(&format!("{BLOBS}/{n}")).map(|b| (n, b)))
        .collect::<Result<_, _>>()?;
    decode_all(files)
}

/// `records` under `set`.
pub fn applied(records: &[FluidRecord], set: DataSet) -> Result<Vec<FluidRecord>, String> {
    records
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r.apply(set).map_err(|e| format!("{}: {e}", r.name))?;
            r.applied.clear();
            Ok(r)
        })
        .collect()
}

pub fn main(args: &[String]) -> ExitCode {
    let repo = Repo::locate();
    let run = || -> Result<Vec<String>, String> {
        let records = records(&repo)?;
        match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
            ["list"] => Ok(records.iter().map(|r| format!("{} ({})", r.name, r.aliases.join(", "))).collect()),
            ["show", name] | ["show", name, "--corrected"] => {
                let set = if args.len() == 3 { DataSet::Corrected } else { DataSet::Parity };
                let record =
                    records.iter().find(|r| r.name.eq_ignore_ascii_case(name)).ok_or(format!("no fluid {name}"))?;
                // The dump is complete: reading it back must give the record (and so the blob) again.
                if from_json(&to_json(record))?.encode() != record.encode() {
                    return Err(format!("{name}: the dump does not read back to its blob"));
                }
                Ok(vec![dump(&applied(std::slice::from_ref(record), set)?[0])])
            }
            ["diff"] => Ok(diff(&applied(&records, DataSet::Parity)?, &applied(&records, DataSet::Corrected)?)),
            ["diff", "--base", rev] => {
                let files = repo.git_files(rev, BLOBS)?;
                Ok(diff(&decode_all(files)?, &records))
            }
            _ => Err("usage: cargo xtask fluid list | show <name> [--corrected] | diff [--base <rev>]".into()),
        }
    };
    match run() {
        Ok(lines) => {
            lines.iter().for_each(|l| println!("{l}"));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fluid: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PLAN.md M2.9a: the dump is complete. For every one of the 136 committed blobs, printing the decoded record and
    /// parsing the text back re-encodes to the blob byte for byte.
    #[test]
    fn dump_round_trips_bitwise() {
        let repo = Repo::locate();
        let names = repo.file_names(BLOBS).unwrap();
        assert_eq!(names.len(), 136);
        for name in names {
            let blob = repo.read_bytes(&format!("{BLOBS}/{name}")).unwrap();
            let record = FluidRecord::decode(&blob).unwrap();
            let text = dump(&record);
            let back = from_json(&serde_json::from_str(&text).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(back.encode(), blob, "{name}");
        }
    }

    /// M2.7's criterion through `fluid diff`: Parity and Corrected differ in exactly the three shipped patches.
    #[test]
    fn parity_and_corrected_diff_is_exactly_the_three_patches() {
        let records = records(&Repo::locate()).unwrap();
        let lines = diff(&applied(&records, DataSet::Parity).unwrap(), &applied(&records, DataSet::Corrected).unwrap());
        assert_eq!(
            lines,
            [
                "Nitrogen: eos.rho_reducing: 11183.901464580624 -> 11183.9",
                "R1234ze(E): eos.gas_constant: 8.314472 -> 8.3144621",
                "Water: melting[3].p0: 623400000.0 -> 632400000.0",
            ]
        );
        assert!(diff(&records, &records).is_empty());
    }

    /// The blob sections the dump covers, by their names in `BLOB_SECTIONS` (PLAN.md §2.5).
    const DUMPED: [&str; 5] = ["metadata", "eos", "superancillary fit", "melting", "corrections"];

    /// PLAN.md §2.5: every section the blob decoder reads has a readable form here; a step that fills a reserved
    /// section (its decoder no longer refuses it) must extend the dump in the same PR, or this fails.
    #[test]
    fn every_blob_section_is_dumped() {
        let read: Vec<&str> = phasekit_core::internal::BLOB_SECTIONS
            .iter()
            .filter(|(_, _, lands_at)| lands_at.is_none())
            .map(|(_, name, _)| *name)
            .collect();
        assert_eq!(
            read, DUMPED,
            "a section the decoder reads is missing from the dump, or the dump lists one it does not"
        );
        let reserved = phasekit_core::internal::BLOB_SECTIONS.len() - read.len();
        assert_eq!(reserved, 5, "superancillary, caloric curves, ancillaries, transport, surface tension");
    }

    /// Every data-terms value and edit kind survives the round trip (the shipped data uses only some of them).
    #[test]
    fn every_terms_value_and_edit_kind_round_trips() {
        let records = records(&Repo::locate()).unwrap();
        let mut r = records.iter().find(|r| r.name == "Water").unwrap().clone();
        for terms in [DataTerms::Unpublished, DataTerms::Restricted, DataTerms::Published] {
            r.source.terms = terms;
            r.corrections = vec![
                Patch { divergence: "DIV-0001".into(), edit: Edit::GasConstant(8.3) },
                Patch { divergence: "DIV-0003".into(), edit: Edit::ReducingDensity(17_000.0) },
                Patch { divergence: "DIV-0006".into(), edit: Edit::MolarMass(0.018) },
                Patch { divergence: "DIV-0002".into(), edit: Edit::MeltingP0 { segment: 3, p0: 632.4e6 } },
            ];
            assert_eq!(from_json(&to_json(&r)).unwrap().encode(), r.encode(), "{terms:?}");
        }
    }

    /// The reader refuses what the form cannot hold, and a fluid present in one set only shows as a whole difference.
    #[test]
    fn malformed_dumps_are_refused() {
        let records = records(&Repo::locate()).unwrap();
        let water = records.iter().find(|r| r.name == "Water").unwrap();
        let good = to_json(water);
        let mut bad = good.clone();
        bad["eos"]["power"][0]["d"] = json!(1.5);
        assert_eq!(from_json(&bad).unwrap_err(), "d: not a small integer");
        let mut bad = good.clone();
        bad["eos"]["ideal"][0]["kind"] = json!("Mystery");
        assert_eq!(from_json(&bad).unwrap_err(), "unknown ideal-gas kind Mystery");
        let mut bad = good.clone();
        bad["corrections"] = json!([{"divergence": "DIV-0002", "edit": {"density": 1.0}}]);
        assert!(from_json(&bad).unwrap_err().starts_with("unknown edit"));
        let mut shorter = water.clone();
        shorter.aliases.pop();
        let lines = diff(std::slice::from_ref(water), std::slice::from_ref(&shorter));
        assert_eq!(lines.len(), 1, "a list of another length differs as a whole: {lines:?}");
        assert!(lines[0].starts_with("Water: aliases: [\"water\",\"WATER\""), "{}", lines[0]);
        let only = diff(&[], std::slice::from_ref(water));
        assert_eq!(only.len(), 1);
        assert!(only[0].starts_with("Water: : null -> {"), "{}", only[0]);
    }
}
