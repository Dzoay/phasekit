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
    Ancillary, CaloricCurves, CaloricStamp, DoubleExponentialTerm, Edit, EosRecord, FluidRecord, GaoBTerm,
    GaussianTerm, IdealTerm, Lemmon2005Term, MeltingForm, MeltingSegment, NonAnalyticTerm, OffsetReference, Part,
    Patch, PowerTerm, PseudoPure, Put, SaStamp, Superancillary, SurfaceTension, Take, TransportRecord,
};
use phasekit_core::{Citation, CitationRole, CriticalOrigin, CriticalPoint, DataSet, DataTerms, Limits, Source};
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
        "source": {
            "bibkey": &*r.source.bibkey,
            "doi": opt_str(r.source.doi.as_deref()),
            "terms": terms_name(r.source.terms),
            "citations": r.source.citations.iter().map(|c| json!({"key": &*c.key, "doi": opt_str(c.doi.as_deref()), "role": role_name(c.role)})).collect::<Vec<_>>(),
        },
        "limits": {"t_min": num(l.t_min()), "t_max": num(l.t_max()), "p_max": num(l.p_max()), "t_triple": l.t_triple().map_or(Value::Null, num)},
        "critical": r.critical.map_or(Value::Null, |c| json!({"t": num(c.t), "p": num(c.p), "rho": num(c.rho), "origin": if c.origin == CriticalOrigin::Model { "Model" } else { "Published" }})),
        "constants": {"acentric": r.acentric.map_or(Value::Null, num), "p_reducing": r.p_reducing.map_or(Value::Null, num), "p_sat_min": r.p_sat_min.map_or(Value::Null, num)},
        "eos": Value::Object(eos),
        "superancillary_fit": r.superancillary_fit.map_or(Value::Null, |s| json!({"shape": format!("{:016x}", s.shape.get()), "gas_constant": num(s.gas_constant), "rho_reducing": num(s.rho_reducing)})),
        "superancillary": r.superancillary.as_ref().map_or(Value::Null, |sa| {
            let list = |v: &[f64]| v.iter().map(|x| num(*x)).collect::<Vec<_>>();
            let pieces = |c: &[[f64; 13]]| c.iter().map(|p| list(p)).collect::<Vec<_>>();
            json!({
                "breaks": list(&sa.breaks),
                "curves": {"rho_l": pieces(&sa.curves[0]), "rho_v": pieces(&sa.curves[1]), "p": pieces(&sa.curves[2])},
                "extrema": {"rho_l": list(&sa.extrema[0]), "rho_v": list(&sa.extrema[1]), "p": list(&sa.extrema[2])},
                "ln_p_breaks": list(&sa.ln_p_breaks),
                "t_of_ln_p": pieces(&sa.t_of_ln_p),
            })
        }),
        "pseudo_pure": r.pseudo_pure.as_ref().map_or(Value::Null, |pp| {
            let one = |a: &Ancillary| json!({
                "exponential": a.exponential, "tau_r": a.tau_r, "t_r": num(a.t_r), "reducing": num(a.reducing),
                "n": a.n.iter().map(|x| num(*x)).collect::<Vec<_>>(), "t": a.t.iter().map(|x| num(*x)).collect::<Vec<_>>(),
                "t_min": num(a.t_min), "t_max": num(a.t_max),
            });
            json!({"p_l": one(&pp.p_l), "p_v": one(&pp.p_v), "rho_l": one(&pp.rho_l), "rho_v": one(&pp.rho_v), "t_range": [num(pp.t_range.0), num(pp.t_range.1)]})
        }),
        "melting": r.melting.iter().map(melting_json).collect::<Vec<_>>(),
        "corrections": r.corrections.iter().map(patch_json).collect::<Vec<_>>(),
        "transport": r.transport.as_ref().map_or(Value::Null, part_json),
        "surface_tension": r.surface_tension.as_ref().map_or(Value::Null, part_json),
        "caloric": r.caloric.as_ref().map_or(Value::Null, |c| {
            let st = &c.stamp;
            let sa = &st.superancillary;
            json!({
                "stamp": {"shape": format!("{:016x}", sa.shape.get()), "gas_constant": num(sa.gas_constant), "rho_reducing": num(sa.rho_reducing), "a1": num(st.a1), "a2": num(st.a2)},
                "breaks": c.breaks.iter().map(|x| num(*x)).collect::<Vec<_>>(),
                "curves": c.curves.iter().map(|curve| curve.iter().map(|p| p.iter().map(|x| num(*x)).collect::<Vec<_>>()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            })
        }),
    })
}

fn role_name(r: CitationRole) -> &'static str {
    match r {
        CitationRole::IdealGas => "IdealGas",
        CitationRole::Erratum => "Erratum",
        CitationRole::Related => "Related",
        _ => "Coefficients",
    }
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
        Edit::EcsQd(q_d) => json!({"q_D": num(q_d)}),
        Edit::RhosrCritical(x) => json!({"rhosr_critical": num(x)}),
        _ => json!({"unknown": null}),
    };
    json!({"divergence": &*p.divergence, "edit": edit})
}

/// Reads the dump back, one section at a time, each with its own parser.
pub fn from_json(v: &Value) -> Result<FluidRecord, String> {
    let eos = eos_from(v.get("eos").ok_or("no eos")?)?;
    let source = source_from(v.get("source").ok_or("no source")?)?;
    let limits = limits_from(v.get("limits").ok_or("no limits")?)?;
    let mut r = FluidRecord::new(&text(v, "name")?, float(v, "molar_mass")?, source, eos, limits);
    r.aliases = list(v, "aliases")?
        .iter()
        .map(|a| a.as_str().map(str::to_string).ok_or("alias: not a string"))
        .collect::<Result<_, _>>()?;
    (r.cas, r.refprop_name, r.inchi_key) = (opt_text(v, "cas"), opt_text(v, "refprop_name"), opt_text(v, "inchi_key"));
    r.critical = section(v, "critical").map(critical_from).transpose()?;
    let constants = v.get("constants").ok_or("no constants")?;
    let constant = |k| section(constants, k).map(|_| float(constants, k)).transpose();
    (r.acentric, r.p_reducing, r.p_sat_min) = (constant("acentric")?, constant("p_reducing")?, constant("p_sat_min")?);
    r.superancillary_fit = section(v, "superancillary_fit").map(fit_from).transpose()?;
    r.pseudo_pure = section(v, "pseudo_pure").map(pseudo_pure_from).transpose()?;
    r.melting = list(v, "melting")?.iter().map(melting_from).collect::<Result<_, _>>()?;
    r.superancillary = section(v, "superancillary").map(superancillary_from).transpose()?;
    r.caloric = section(v, "caloric").map(caloric_from).transpose()?;
    r.corrections = list(v, "corrections")?.iter().map(patch_from).collect::<Result<_, _>>()?;
    if let Some(t) = section(v, "transport") {
        r.transport = Some(TransportRecord::take(&mut JsonTake(vec![t])).map_err(|e| format!("transport: {e}"))?);
    }
    let fit = section(v, "surface_tension").map(|s| SurfaceTension::take(&mut JsonTake(vec![s])));
    r.surface_tension = fit.transpose().map_err(|e| format!("surface_tension: {e}"))?;
    Ok(r)
}

/// A part of the record written through core's description of it ([`Part`]).
fn part_json(part: &impl Part) -> Value {
    let mut out = JsonPut(vec![("part", Map::new())]);
    part.put(&mut out);
    out.0.pop().map_or(Value::Null, |(_, map)| Value::Object(map))
}

/// The `transport` section as JSON (M8.1), written through core's one description of the record ([`Part`]): a part
/// is an object, a variant or hardcoded model its name, an absent optional field or part `null`.
struct JsonPut(Vec<(&'static str, Map<String, Value>)>);

impl JsonPut {
    fn insert(&mut self, name: &str, value: Value) {
        if let Some((_, map)) = self.0.last_mut() {
            map.insert(name.into(), value);
        }
    }
}

impl Put for JsonPut {
    fn num(&mut self, name: &'static str, x: f64) {
        self.insert(name, num(x));
    }
    fn nums(&mut self, name: &'static str, xs: &[f64]) {
        self.insert(name, xs.iter().map(|x| num(*x)).collect());
    }
    fn small(&mut self, name: &'static str, x: u8) {
        self.insert(name, x.into());
    }
    fn text(&mut self, name: &'static str, x: &str) {
        self.insert(name, x.into());
    }
    fn choice(&mut self, name: &'static str, names: &'static [&'static str], index: usize) {
        self.insert(name, opt_str(names.get(index).copied()));
    }
    fn present(&mut self, name: &'static str, present: bool) {
        if !present {
            self.insert(name, Value::Null);
        }
    }
    fn open(&mut self, name: &'static str) {
        self.0.push((name, Map::new()));
    }
    fn close(&mut self) {
        if let Some((name, map)) = self.0.pop() {
            self.insert(name, Value::Object(map));
        }
    }
}

/// Reads what [`JsonPut`] wrote.
struct JsonTake<'v>(Vec<&'v Value>);

impl<'v> JsonTake<'v> {
    fn field(&self, name: &str) -> Result<&'v Value, String> {
        self.0.last().and_then(|v| v.get(name)).ok_or(format!("{name}: missing"))
    }
}

impl Take for JsonTake<'_> {
    fn num(&mut self, name: &'static str) -> Result<f64, String> {
        self.field(name)?.as_f64().ok_or(format!("{name}: not a number"))
    }
    fn nums(&mut self, name: &'static str) -> Result<Vec<f64>, String> {
        let list = self.field(name)?.as_array().ok_or(format!("{name}: not a list"))?;
        list.iter().map(|x| x.as_f64().ok_or(format!("{name}: {x} is not a number"))).collect()
    }
    fn small(&mut self, name: &'static str) -> Result<u8, String> {
        let x = self.field(name)?.as_u64().and_then(|x| u8::try_from(x).ok());
        x.ok_or(format!("{name}: not a small integer"))
    }
    fn text(&mut self, name: &'static str) -> Result<String, String> {
        self.field(name)?.as_str().map(str::to_string).ok_or(format!("{name}: not a string"))
    }
    fn choice(&mut self, name: &'static str, names: &'static [&'static str]) -> Result<usize, String> {
        let chosen = self.field(name)?.as_str().ok_or(format!("{name}: not a string"))?;
        names.iter().position(|n| *n == chosen).ok_or(format!("{name}: unknown {chosen:?}"))
    }
    fn present(&mut self, name: &'static str) -> Result<bool, String> {
        Ok(self.0.last().and_then(|v| v.get(name)).is_some_and(|x| !x.is_null()))
    }
    fn open(&mut self, name: &'static str) -> Result<(), String> {
        let part = self.field(name)?;
        self.0.push(part);
        Ok(())
    }
    fn close(&mut self) -> Result<(), String> {
        self.0.pop().map(drop).ok_or("close without open".into())
    }
}

/// The number at `k` of `v`.
fn float(v: &Value, k: &str) -> Result<f64, String> {
    v.get(k).and_then(Value::as_f64).ok_or(format!("{k}: not a number"))
}

/// The integer at `k` of `v`, which fits a byte.
fn small(v: &Value, k: &str) -> Result<u8, String> {
    v.get(k).and_then(Value::as_u64).and_then(|x| u8::try_from(x).ok()).ok_or(format!("{k}: not a small integer"))
}

/// The string at `k` of `v`.
fn text(v: &Value, k: &str) -> Result<String, String> {
    v.get(k).and_then(Value::as_str).map(str::to_string).ok_or(format!("{k}: not a string"))
}

/// The string at `k` of `v`, if it has one.
fn opt_text(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

/// The list at `k` of `v`.
fn list(v: &Value, k: &str) -> Result<Vec<Value>, String> {
    v.get(k).and_then(Value::as_array).cloned().ok_or(format!("{k}: not a list"))
}

/// The section at `k` of `v`, unless it is absent or null.
fn section<'v>(v: &'v Value, k: &str) -> Option<&'v Value> {
    v.get(k).filter(|x| !x.is_null())
}

/// The `eos` section: the reducing state and ρ_max, then each kind's terms.
fn eos_from(e: &Value) -> Result<EosRecord, String> {
    let (f, u) = (float, small);
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
        eos.ideal.push(ideal_from(&t)?);
    }
    Ok(eos)
}

/// One ideal-gas term of the `eos` section, by its `kind`.
fn ideal_from(t: &Value) -> Result<IdealTerm, String> {
    let f = float;
    Ok(match text(t, "kind")?.as_str() {
        "Lead" => IdealTerm::Lead { a1: f(t, "a1")?, a2: f(t, "a2")? },
        "LogTau" => IdealTerm::LogTau { a: f(t, "a")? },
        "Power" => IdealTerm::Power { n: f(t, "n")?, t: f(t, "t")? },
        "PlanckEinstein" => IdealTerm::PlanckEinstein { n: f(t, "n")?, theta: f(t, "theta")? },
        "PlanckEinsteinGeneralized" => {
            IdealTerm::PlanckEinsteinGeneralized { n: f(t, "n")?, theta: f(t, "theta")?, c: f(t, "c")?, d: f(t, "d")? }
        }
        "Cp0Power" => IdealTerm::Cp0Power { c: f(t, "c")?, t: f(t, "t")?, tc: f(t, "tc")?, t0: f(t, "t0")? },
        "Offset" => {
            let reference = match text(t, "reference")?.as_str() {
                "IIR" => OffsetReference::Iir,
                "NBP" => OffsetReference::Nbp,
                "OTH" => OffsetReference::Other,
                "CUSTOM" => OffsetReference::Custom,
                other => return Err(format!("unknown offset reference {other}")),
            };
            IdealTerm::Offset { a1: f(t, "a1")?, a2: f(t, "a2")?, reference }
        }
        other => return Err(format!("unknown ideal-gas kind {other}")),
    })
}

/// The `source` section: the data's terms, its bibkey and DOI, then its citations.
fn source_from(src: &Value) -> Result<Source, String> {
    let terms = match text(src, "terms")?.as_str() {
        "Published" => DataTerms::Published,
        "Unpublished" => DataTerms::Unpublished,
        "Restricted" => DataTerms::Restricted,
        other => return Err(format!("unknown data terms {other}")),
    };
    let mut source = Source::new(&text(src, "bibkey")?, opt_text(src, "doi").as_deref(), terms);
    for c in list(src, "citations")? {
        let role = match text(&c, "role")?.as_str() {
            "Coefficients" => CitationRole::Coefficients,
            "IdealGas" => CitationRole::IdealGas,
            "Erratum" => CitationRole::Erratum,
            "Related" => CitationRole::Related,
            other => return Err(format!("unknown citation role {other}")),
        };
        source.citations.push(Citation {
            key: text(&c, "key")?.into(),
            doi: opt_text(&c, "doi").map(Into::into),
            role,
        });
    }
    Ok(source)
}

/// The `limits` section: the range in T, p_max and, where it has one, the triple point.
fn limits_from(lim: &Value) -> Result<Limits, String> {
    let limits =
        Limits::new(float(lim, "t_min")?, float(lim, "t_max")?, float(lim, "p_max")?).map_err(|e| e.to_string())?;
    Ok(match lim.get("t_triple").and_then(Value::as_f64) {
        Some(t) => limits.with_t_triple(t),
        None => limits,
    })
}

/// The `critical` section.
fn critical_from(c: &Value) -> Result<CriticalPoint, String> {
    let origin = if text(c, "origin")? == "Model" { CriticalOrigin::Model } else { CriticalOrigin::Published };
    Ok(CriticalPoint { t: float(c, "t")?, p: float(c, "p")?, rho: float(c, "rho")?, origin })
}

/// The `superancillary_fit` section: the stamp of the EOS the superancillary was fitted to.
fn fit_from(fit: &Value) -> Result<SaStamp, String> {
    let shape = u64::from_str_radix(&text(fit, "shape")?, 16).map_err(|e| format!("shape: {e}"))?;
    let shape = phasekit_core::internal::model_key(shape);
    Ok(SaStamp { shape, gas_constant: float(fit, "gas_constant")?, rho_reducing: float(fit, "rho_reducing")? })
}

/// The `pseudo_pure` section: its range in T, then its four ancillaries.
fn pseudo_pure_from(pp: &Value) -> Result<PseudoPure, String> {
    let one = |key: &str| ancillary_from(pp.get(key).ok_or(format!("pseudo_pure: no {key}"))?);
    let range = pseudo_pure_floats(pp, "t_range")?;
    let &[lo, hi] = range.as_slice() else { return Err("pseudo_pure: t_range needs two temperatures".into()) };
    Ok(PseudoPure { p_l: one("p_l")?, p_v: one("p_v")?, rho_l: one("rho_l")?, rho_v: one("rho_v")?, t_range: (lo, hi) })
}

/// One ancillary of the `pseudo_pure` section.
fn ancillary_from(a: &Value) -> Result<Ancillary, String> {
    let flag = |key: &str| a.get(key).and_then(Value::as_bool).ok_or(format!("pseudo_pure: no {key}"));
    Ok(Ancillary {
        exponential: flag("exponential")?,
        tau_r: flag("tau_r")?,
        t_r: float(a, "t_r")?,
        reducing: float(a, "reducing")?,
        n: pseudo_pure_floats(a, "n")?,
        t: pseudo_pure_floats(a, "t")?,
        t_min: float(a, "t_min")?,
        t_max: float(a, "t_max")?,
    })
}

/// The numbers listed at `key` of a value in the `pseudo_pure` section.
fn pseudo_pure_floats(x: &Value, key: &str) -> Result<Vec<f64>, String> {
    let items = x.get(key).and_then(Value::as_array).ok_or(format!("pseudo_pure: no {key}"))?;
    items.iter().map(|x| x.as_f64().ok_or_else(|| format!("pseudo_pure {key}: not a number"))).collect()
}

/// One segment of the `melting` section: its form ("simon" with a and c, or "in_tr" or "in_theta" with its [aᵢ, tᵢ]
/// terms), reference point and range.
fn melting_json(s: &MeltingSegment) -> Value {
    let terms = |terms: &[(f64, f64)]| terms.iter().map(|&(a, t)| json!([num(a), num(t)])).collect::<Vec<_>>();
    let mut out = match &s.form {
        MeltingForm::Simon { a, c } => json!({"form": "simon", "a": num(*a), "c": num(*c)}),
        MeltingForm::PolynomialInTr(t) => json!({"form": "in_tr", "terms": terms(t)}),
        MeltingForm::PolynomialInTheta(t) => json!({"form": "in_theta", "terms": terms(t)}),
    };
    for (key, x) in [("t0", s.t0), ("p0", s.p0), ("t_min", s.t_min), ("t_max", s.t_max)] {
        out[key] = num(x);
    }
    out
}

/// One segment of the `melting` section, as [`melting_json`] writes it.
fn melting_from(m: &Value) -> Result<MeltingSegment, String> {
    let f = float;
    let terms = || -> Result<Box<[(f64, f64)]>, String> {
        let items = m.get("terms").and_then(Value::as_array).ok_or("melting: no terms")?;
        let pair = |x: &Value| match x
            .as_array()
            .map(|p| (p.first().and_then(Value::as_f64), p.get(1).and_then(Value::as_f64)))
        {
            Some((Some(a), Some(t))) => Ok((a, t)),
            _ => Err("melting: a term is not [a, t]".to_string()),
        };
        items.iter().map(pair).collect()
    };
    let form = match m.get("form").and_then(Value::as_str) {
        Some("simon") => MeltingForm::Simon { a: f(m, "a")?, c: f(m, "c")? },
        Some("in_tr") => MeltingForm::PolynomialInTr(terms()?),
        Some("in_theta") => MeltingForm::PolynomialInTheta(terms()?),
        other => return Err(format!("melting: form {other:?}")),
    };
    Ok(MeltingSegment { form, t0: f(m, "t0")?, p0: f(m, "p0")?, t_min: f(m, "t_min")?, t_max: f(m, "t_max")? })
}

/// The `superancillary` section: the breaks, the three curves and their extrema, and the inverse T(ln p).
fn superancillary_from(sa: &Value) -> Result<Superancillary, String> {
    let floats = |x: &Value| -> Result<Vec<f64>, String> {
        let items = x.as_array().ok_or("superancillary: not a list")?;
        items.iter().map(|x| x.as_f64().ok_or_else(|| "superancillary: not a number".to_string())).collect()
    };
    let pieces = |x: &Value| -> Result<Vec<[f64; 13]>, String> {
        let items = x.as_array().ok_or("superancillary: not a list of pieces")?;
        items
            .iter()
            .map(|p| floats(p)?.try_into().map_err(|_| "superancillary: a piece needs 13 coefficients".to_string()))
            .collect()
    };
    let field = |x: &Value, key: &str| x.get(key).cloned().ok_or_else(|| format!("superancillary without {key}"));
    let named = |x: &Value, key: &str| {
        field(x, key)
            .and_then(|v| ["rho_l", "rho_v", "p"].map(|k| field(&v, k)).into_iter().collect::<Result<Vec<_>, _>>())
    };
    let [l, v_, p] = <[Value; 3]>::try_from(named(sa, "curves")?).map_err(|_| "superancillary: three curves")?;
    let [el, ev, ep] =
        <[Value; 3]>::try_from(named(sa, "extrema")?).map_err(|_| "superancillary: three extrema lists")?;
    Ok(Superancillary {
        breaks: floats(&field(sa, "breaks")?)?,
        curves: [pieces(&l)?, pieces(&v_)?, pieces(&p)?],
        extrema: [floats(&el)?, floats(&ev)?, floats(&ep)?],
        ln_p_breaks: floats(&field(sa, "ln_p_breaks")?)?,
        t_of_ln_p: pieces(&field(sa, "t_of_ln_p")?)?,
    })
}

/// The `caloric` section: the stamp of what the curves were fitted to, the breaks and the six curves.
fn caloric_from(c: &Value) -> Result<CaloricCurves, String> {
    let st = c.get("stamp").ok_or("caloric without a stamp")?;
    let shape = u64::from_str_radix(&text(st, "shape")?, 16).map_err(|e| format!("caloric shape: {e}"))?;
    let superancillary = SaStamp {
        shape: phasekit_core::internal::model_key(shape),
        gas_constant: float(st, "gas_constant")?,
        rho_reducing: float(st, "rho_reducing")?,
    };
    let stamp = CaloricStamp { superancillary, a1: float(st, "a1")?, a2: float(st, "a2")? };
    let floats = |x: &Value| x.as_f64().ok_or("caloric: not a number");
    let breaks = list(c, "breaks")?.iter().map(floats).collect::<Result<Vec<_>, _>>()?;
    let mut curves: [Vec<[f64; 13]>; 6] = Default::default();
    let given = list(c, "curves")?;
    if given.len() != 6 {
        return Err(format!("caloric: {} curves, not 6", given.len()));
    }
    for (curve, pieces) in curves.iter_mut().zip(&given) {
        for piece in pieces.as_array().ok_or("caloric: a curve is not a list")? {
            let row = piece.as_array().ok_or("caloric: a piece is not a list")?;
            let row: Vec<f64> = row.iter().map(floats).collect::<Result<_, _>>()?;
            curve.push(row.try_into().map_err(|_| "caloric: a piece needs 13 coefficients")?);
        }
    }
    Ok(CaloricCurves { stamp, breaks, curves })
}

/// One correction of the `corrections` section: its divergence and its edit.
fn patch_from(p: &Value) -> Result<Patch, String> {
    let edit = p.get("edit").ok_or("correction without an edit")?;
    let edit = if let Some(x) = edit.get("gas_constant").and_then(Value::as_f64) {
        Edit::GasConstant(x)
    } else if let Some(x) = edit.get("rho_reducing").and_then(Value::as_f64) {
        Edit::ReducingDensity(x)
    } else if let Some(x) = edit.get("molar_mass").and_then(Value::as_f64) {
        Edit::MolarMass(x)
    } else if let Some(m) = edit.get("melting_p0") {
        Edit::MeltingP0 { segment: small(m, "segment")?, p0: float(m, "p0")? }
    } else if let Some(x) = edit.get("q_D").and_then(Value::as_f64) {
        Edit::EcsQd(x)
    } else if let Some(x) = edit.get("rhosr_critical").and_then(Value::as_f64) {
        Edit::RhosrCritical(x)
    } else {
        return Err(format!("unknown edit {edit}"));
    };
    Ok(Patch { divergence: text(p, "divergence")?.into(), edit })
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

/// `fluid diff --base <rev>`: the blobs of `rev` against `records`. Blobs in another format version (the change itself
/// bumped it) cannot be decoded field by field: that is reported as one line, not as a failure.
pub fn diff_base(rev: &str, files: Vec<(String, Vec<u8>)>, records: &[FluidRecord]) -> Result<Vec<String>, String> {
    match decode_all(files) {
        Ok(old) => Ok(diff(&old, records)),
        Err(e) if e.contains("bad data: blob version") => {
            Ok(vec![format!("{rev}: the blob format changed ({e}); no field-by-field diff")])
        }
        Err(e) => Err(e),
    }
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
            ["diff", "--base", rev] => diff_base(rev, repo.git_files(rev, BLOBS)?, &records),
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
    const DUMPED: [&str; 10] = [
        "metadata",
        "eos",
        "superancillary fit",
        "superancillary",
        "caloric curves",
        "ancillaries",
        "transport",
        "surface tension",
        "melting",
        "corrections",
    ];

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
        assert_eq!(read.len(), phasekit_core::internal::BLOB_SECTIONS.len(), "every section of v1 is filled");
    }

    /// The transport section writes an absent model, part or field as `null`, as the other sections do (M8.1); a
    /// viscosity always has Lennard-Jones parameters, its own or Chung's (M8.9).
    #[test]
    fn absent_transport_parts_dump_as_null() {
        let records = records(&Repo::locate()).unwrap();
        let transport = |name: &str| to_json(records.iter().find(|r| r.name == name).unwrap())["transport"].clone();
        assert_eq!(transport("Acetone"), json!({"viscosity": null, "conductivity": null}));
        let ethylbenzene = transport("EthylBenzene");
        assert_eq!(ethylbenzene["viscosity"]["source"].get("doi"), Some(&Value::Null), "Huber-RP912 has no DOI");
        let water = transport("Water");
        assert_eq!(water["viscosity"]["lennard_jones"]["kind"], json!("Chung"), "named, never null (M8.9)");
        assert!(water["viscosity"]["source"]["doi"].is_string());
    }

    /// Caloric curves survive the dump (no shipped blob has them before M5.2a).
    #[test]
    fn caloric_curves_round_trip() {
        let records = records(&Repo::locate()).unwrap();
        let mut r = records.iter().find(|r| r.name == "Nitrogen").unwrap().clone();
        let superancillary = r.superancillary_fit.unwrap();
        let rows = |k: f64| vec![core::array::from_fn(|i| k + i as f64), core::array::from_fn(|i| -k * i as f64)];
        let curves = core::array::from_fn(|c| rows(c as f64 + 0.5));
        r.caloric = Some(CaloricCurves {
            stamp: CaloricStamp { superancillary, a1: 0.1, a2: -0.2 },
            breaks: vec![63.2, 100.0, 126.1],
            curves,
        });
        assert_eq!(from_json(&to_json(&r)).unwrap().encode(), r.encode());
        let mut bad = to_json(&r);
        bad["caloric"]["curves"][2][0] = json!([1.0, 2.0]);
        assert_eq!(from_json(&bad).unwrap_err(), "caloric: a piece needs 13 coefficients");
    }

    /// A base revision whose blobs use another format version is reported in one line; any other decode failure
    /// is an error.
    #[test]
    fn diff_against_another_format_version_is_one_line() {
        let repo = Repo::locate();
        let name = "water.bin".to_string();
        let blob = repo.read_bytes(&format!("{BLOBS}/{name}")).unwrap();
        let records = records(&repo).unwrap();
        assert!(
            diff_base("main", vec![(name.clone(), blob.clone())], &records).unwrap().len() > 100,
            "the other fluids"
        );
        let mut old = blob.clone();
        old[8] = 1;
        let lines = diff_base("main", vec![(name.clone(), old)], &records).unwrap();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("main: the blob format changed (water.bin: bad data: blob version 1;"),
            "{}",
            lines[0]
        );
        let mut corrupt = blob;
        corrupt[40] ^= 1;
        assert!(diff_base("main", vec![(name, corrupt)], &records).is_err());
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
                Patch { divergence: "DIV-0015".into(), edit: Edit::EcsQd(2e9) },
                Patch { divergence: "DIV-0016".into(), edit: Edit::RhosrCritical(-54_095.5) },
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
