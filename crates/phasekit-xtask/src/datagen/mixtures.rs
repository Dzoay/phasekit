//! `cargo xtask datagen`'s mixture step (PLAN.md M13.2): the pinned v8.0.0 mixture library (`dev/mixtures`) →
//! `phasekit-mix`'s generated tables. The files are checked against `data/mixtures.lock` first, then read through a
//! closed serde mirror (every struct denies unknown fields) and checked:
//! - the 888 binary pairs, keyed by their CAS pair sorted as strings, as v8.0.0 orients them
//!   (`MixtureParameters.cpp:130-163`): a swap inverts β_T and β_v, and Lemmon's ξ and ζ are symmetric. A pair gives
//!   GERG's β/γ or Lemmon's ξ/ζ, never both, and carries a departure function exactly when F ≠ 0 (v8.0.0 reads the
//!   function only then, `MixtureParameters.cpp:145-147`). No CAS pair appears twice.
//! - the 28 departure functions, by kind: Exponential, GERG-2008 and Gaussian + Exponential (`ExcessHEFunction.h`).
//! - the predefined mixtures, as mole fractions: an entry that repeats an earlier one exactly is dropped (seven do), one
//!   that differs is refused (ROT-041). Where ASHRAE 34's list (`ASHRAE_predefined_2026.tsv`, mass percent) defines a
//!   blend, its mass fractions are kept beside v8.0.0's mole fractions, in the entry's component order (ROT-199).
//! - every v8.0.0 fluid's CAS number, by canonical name: a component's key into the pairs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Deserialize;

use super::Source;
use crate::repo::Repo;

/// The pinned mixture files (gitignored; `scripts/fetch-coolprop.sh`).
pub const MIXTURES_DIR: &str = "reference/CoolProp/dev/mixtures";

/// Their sha256 lines.
pub const MIXTURES_LOCK: &str = "data/mixtures.lock";

/// The files the step reads, in the lock's order.
pub const FILES: [&str; 4] = [
    "ASHRAE_predefined_2026.tsv",
    "mixture_binary_pairs.json",
    "mixture_departure_functions.json",
    "predefined_mixtures.json",
];

/// The generated file.
pub const GENERATED: &str = "crates/phasekit-mix/src/generated.rs";

/// One binary pair of `mixture_binary_pairs.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairJson {
    #[serde(rename = "BibTeX")]
    pub bibtex: String,
    #[serde(rename = "CAS1")]
    pub cas1: String,
    #[serde(rename = "CAS2")]
    pub cas2: String,
    #[serde(rename = "F")]
    pub f: f64,
    #[serde(rename = "Name1")]
    pub name1: String,
    #[serde(rename = "Name2")]
    pub name2: String,
    #[serde(rename = "betaT")]
    pub beta_t: Option<f64>,
    #[serde(rename = "betaV")]
    pub beta_v: Option<f64>,
    #[serde(rename = "gammaT")]
    pub gamma_t: Option<f64>,
    #[serde(rename = "gammaV")]
    pub gamma_v: Option<f64>,
    pub xi: Option<f64>,
    pub zeta: Option<f64>,
    pub function: Option<String>,
}

/// One departure function of `mixture_departure_functions.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepartureJson {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "BibTeX")]
    pub bibtex: String,
    pub aliases: Vec<String>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "Npower")]
    pub npower: Option<usize>,
    pub n: Vec<f64>,
    pub t: Vec<f64>,
    pub d: Vec<f64>,
    pub l: Option<Vec<f64>>,
    pub eta: Option<Vec<f64>>,
    pub epsilon: Option<Vec<f64>>,
    pub beta: Option<Vec<f64>>,
    pub gamma: Option<Vec<f64>>,
}

/// One predefined mixture of `predefined_mixtures.json`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PredefinedJson {
    pub name: String,
    pub fluids: Vec<String>,
    pub mole_fractions: Vec<f64>,
}

/// A binary pair's reducing parameters, oriented to its sorted CAS pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reducing {
    /// GERG's β_T, γ_T, β_v, γ_v.
    Gerg([f64; 4]),
    /// Lemmon's ξ (K) and ζ (m³/mol), converted to γ_T and γ_v from the components' reducing states when a model is
    /// built (`ReducingFunctions.h:695-706`).
    Lemmon([f64; 2]),
}

/// A binary pair as generated.
#[derive(Clone, Debug, PartialEq)]
pub struct Pair {
    pub cas: [String; 2],
    pub names: [String; 2],
    pub reducing: Reducing,
    pub f: f64,
    /// The index of its departure function, where F ≠ 0.
    pub departure: Option<usize>,
    pub source: String,
}

/// A departure function's kind, as v8.0.0 evaluates it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Σ n δ^d τ^t exp(−δ^l), no exponential where l = 0 (`ExcessHEFunction.h:182-196`).
    Exponential,
    /// `power` terms n δ^d τ^t, then n δ^d τ^t exp(−η(δ − ε)² − β(δ − γ)) (`ExcessHEFunction.h:104-134`).
    Gerg2008,
    /// `power` terms n δ^d τ^t exp(−δ^l), then n δ^d τ^t exp(−η(δ − ε)² − β(τ − γ)²) (`ExcessHEFunction.h:143-172`).
    GaussianExponential,
}

/// One term: the fields a kind does not read are 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Term {
    pub n: f64,
    pub t: f64,
    pub d: f64,
    pub l: f64,
    pub eta: f64,
    pub epsilon: f64,
    pub beta: f64,
    pub gamma: f64,
}

/// A departure function as generated.
#[derive(Clone, Debug, PartialEq)]
pub struct Departure {
    pub name: String,
    pub aliases: Vec<String>,
    pub source: String,
    pub kind: Kind,
    /// The leading terms without a Gaussian (GERG-2008, Gaussian + Exponential); every term for Exponential.
    pub power: usize,
    pub terms: Vec<Term>,
}

/// A predefined mixture as generated: v8.0.0's components and mole fractions, and ASHRAE 34's mass fractions where its
/// list defines the blend.
#[derive(Clone, Debug, PartialEq)]
pub struct Predefined {
    pub name: String,
    pub components: Vec<String>,
    pub mole_fractions: Vec<f64>,
    pub mass_fractions: Option<Vec<f64>>,
}

/// The library, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct Library {
    pub pairs: Vec<Pair>,
    pub departures: Vec<Departure>,
    pub predefined: Vec<Predefined>,
    /// Every fluid's (canonical name, CAS), by name.
    pub cas: Vec<(String, String)>,
    /// What datagen logs: duplicates dropped, fields ignored.
    pub notes: Vec<String>,
}

/// Checks the files against the lock: each named once, with the lock's sha256.
pub fn check_lock(lock: &str, files: &[(&str, &str)]) -> Result<(), Vec<String>> {
    let locked: Vec<(&str, &str)> = lock
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_once(' '))
        .collect();
    let mut errors = Vec::new();
    for (name, text) in files {
        match locked.iter().find(|(f, _)| f == name) {
            None => errors.push(format!("{name}: not in {MIXTURES_LOCK}")),
            Some((_, sha)) if *sha != phasekit_verify::sha256_hex(text.as_bytes()) => {
                errors.push(format!("{name}: sha256 differs from {MIXTURES_LOCK} ({sha})"));
            }
            Some(_) => {}
        }
    }
    for (name, _) in locked.iter().filter(|(f, _)| !files.iter().any(|(n, _)| n == f)) {
        errors.push(format!("{name}: in {MIXTURES_LOCK}, not read"));
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// The pinned files, after the lock check, in [`FILES`] order.
pub fn read(repo: &Repo) -> Result<Vec<String>, Vec<String>> {
    let texts = FILES
        .iter()
        .map(|file| repo.read(&format!("{MIXTURES_DIR}/{file}")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| vec![format!("{e} (scripts/fetch-coolprop.sh)")])?;
    let named: Vec<(&str, &str)> = FILES.iter().copied().zip(texts.iter().map(String::as_str)).collect();
    check_lock(&repo.read(MIXTURES_LOCK).map_err(|e| vec![e])?, &named)?;
    Ok(texts)
}

/// The library from the pinned files and the parsed fluid sources.
pub fn load(repo: &Repo, fluids: &[Source]) -> Result<Library, Vec<String>> {
    let texts = read(repo)?;
    let [ashrae, pairs, departures, predefined] = &texts[..] else {
        return Err(vec![format!("{MIXTURES_DIR}: {} files read, {} expected", texts.len(), FILES.len())]);
    };
    library(pairs, departures, predefined, ashrae, fluids)
}

/// The library from the texts of the four files.
pub fn library(
    pairs: &str,
    departures: &str,
    predefined: &str,
    ashrae: &str,
    fluids: &[Source],
) -> Result<Library, Vec<String>> {
    let parse = |file: &str, e: serde_json::Error| vec![format!("{file}: {e}")];
    let pairs: Vec<PairJson> = serde_json::from_str(pairs).map_err(|e| parse(FILES[1], e))?;
    let departures: Vec<DepartureJson> = serde_json::from_str(departures).map_err(|e| parse(FILES[2], e))?;
    let predefined: Vec<PredefinedJson> = serde_json::from_str(predefined).map_err(|e| parse(FILES[3], e))?;
    let mut notes = Vec::new();
    let departures = departures.iter().map(|d| departure(d, &mut notes)).collect::<Result<Vec<_>, _>>()?;
    let pairs = oriented_pairs(&pairs, &departures)?;
    let predefined = predefined_mixtures(&predefined, &parse_ashrae(ashrae)?, fluids, &mut notes)?;
    let mut cas: Vec<(String, String)> =
        fluids.iter().map(|s| (s.fluid.info.name.clone(), s.fluid.info.cas.clone())).collect();
    cas.sort();
    Ok(Library { pairs, departures, predefined, cas, notes })
}

/// A pair, oriented to its sorted CAS pair, with its departure's index; or what is wrong with it.
fn oriented(p: &PairJson, departures: &[Departure]) -> Result<Pair, String> {
    // JSON numbers are finite, so only the signs need checking: β divides on a swap.
    let who = format!("{}/{} ({}/{})", p.name1, p.name2, p.cas1, p.cas2);
    let reducing = match (p.beta_t, p.gamma_t, p.beta_v, p.gamma_v, p.xi, p.zeta) {
        (Some(bt), Some(gt), Some(bv), Some(gv), None, None) if bt > 0.0 && bv > 0.0 => {
            Reducing::Gerg([bt, gt, bv, gv])
        }
        (None, None, None, None, Some(xi), Some(zeta)) => Reducing::Lemmon([xi, zeta]),
        _ => {
            return Err(format!(
                "{who}: needs GERG's betaT, gammaT, betaV, gammaV (β > 0) or Lemmon's xi, zeta, not both"
            ));
        }
    };
    let departure = match (p.f != 0.0, &p.function) {
        (true, Some(name)) => {
            Some(departures.iter().position(|d| d.name == *name).ok_or_else(|| format!("{who}: no departure {name}"))?)
        }
        (true, None) => return Err(format!("{who}: F = {} without a departure function", p.f)),
        (false, Some(name)) => return Err(format!("{who}: departure {name} with F = 0")),
        (false, None) => None,
    };
    let swap = match p.cas1.cmp(&p.cas2) {
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => return Err(format!("{who}: a pair needs two components")),
    };
    let (cas, names) = if swap {
        ([p.cas2.clone(), p.cas1.clone()], [p.name2.clone(), p.name1.clone()])
    } else {
        ([p.cas1.clone(), p.cas2.clone()], [p.name1.clone(), p.name2.clone()])
    };
    let reducing = match reducing {
        Reducing::Gerg([bt, gt, bv, gv]) if swap => Reducing::Gerg([1.0 / bt, gt, 1.0 / bv, gv]),
        other => other,
    };
    Ok(Pair { cas, names, reducing, f: p.f, departure, source: p.bibtex.clone() })
}

/// Every pair oriented, sorted by CAS pair; a CAS pair given twice is refused.
fn oriented_pairs(pairs: &[PairJson], departures: &[Departure]) -> Result<Vec<Pair>, Vec<String>> {
    let (ok, errors): (Vec<_>, Vec<_>) = pairs.iter().map(|p| oriented(p, departures)).partition(Result::is_ok);
    let mut errors: Vec<String> = errors.into_iter().filter_map(Result::err).collect();
    let mut pairs: Vec<Pair> = ok.into_iter().filter_map(Result::ok).collect();
    pairs.sort_by(|a, b| a.cas.cmp(&b.cas));
    for w in pairs.windows(2) {
        if let [a, b] = w
            && a.cas == b.cas
        {
            errors.push(format!("{}/{}: the CAS pair {:?} is given twice", a.names[0], a.names[1], a.cas));
        }
    }
    if errors.is_empty() { Ok(pairs) } else { Err(errors) }
}

/// A departure function, checked: its kind, equal lengths, integer exponents d and l, and `Npower` within the terms.
fn departure(d: &DepartureJson, notes: &mut Vec<String>) -> Result<Departure, Vec<String>> {
    let who = &d.name;
    let fail = |what: String| vec![format!("departure {who}: {what}")];
    let len = d.n.len();
    let column = |name: &str, v: &Option<Vec<f64>>| -> Result<Vec<f64>, Vec<String>> {
        match v {
            Some(v) if v.len() == len => Ok(v.clone()),
            Some(v) => Err(fail(format!("{name} has {} entries, n has {len}", v.len()))),
            None => Err(fail(format!("no {name}"))),
        }
    };
    if d.t.len() != len || d.d.len() != len {
        return Err(fail("t, d and n differ in length".into()));
    }
    let zeros = vec![0.0; len];
    let (kind, power, l, gauss) = match d.kind.as_str() {
        "Exponential" => {
            if d.npower.is_some() {
                notes
                    .push(format!("departure {who}: Npower ignored, as v8.0.0 ignores it for an Exponential function"));
            }
            (Kind::Exponential, len, column("l", &d.l)?, None)
        }
        "GERG-2008" => {
            let gauss = [column("eta", &d.eta)?, column("epsilon", &d.epsilon)?, column("beta", &d.beta)?];
            (
                Kind::Gerg2008,
                d.npower.ok_or_else(|| fail("no Npower".into()))?,
                zeros,
                Some((gauss, column("gamma", &d.gamma)?)),
            )
        }
        "Gaussian+Exponential" => {
            let gauss = [column("eta", &d.eta)?, column("epsilon", &d.epsilon)?, column("beta", &d.beta)?];
            let power = d.npower.ok_or_else(|| fail("no Npower".into()))?;
            (Kind::GaussianExponential, power, column("l", &d.l)?, Some((gauss, column("gamma", &d.gamma)?)))
        }
        other => return Err(fail(format!("unknown type {other}"))),
    };
    if power > len || d.d.iter().chain(&l).any(|x| x.fract() != 0.0 || *x < 0.0) {
        return Err(fail(format!("Npower {power} beyond {len} terms, or a d or l that is not a whole number ≥ 0")));
    }
    let terms = (0..len)
        .map(|i| {
            let at = |v: &[f64]| v.get(i).copied().unwrap_or(0.0);
            let ([eta, epsilon, beta], gamma) =
                gauss.as_ref().map_or(([0.0; 3], 0.0), |(g, gamma)| ([at(&g[0]), at(&g[1]), at(&g[2])], at(gamma)));
            Term { n: at(&d.n), t: at(&d.t), d: at(&d.d), l: at(&l), eta, epsilon, beta, gamma }
        })
        .collect();
    let (name, aliases, source) = (d.name.clone(), d.aliases.clone(), d.bibtex.clone());
    Ok(Departure { name, aliases, source, kind, power, terms })
}

/// One ASHRAE 34 row: the blend's name and its components with their mass fractions.
pub type AshraeRow = (String, Vec<(String, f64)>);

/// ASHRAE 34's rows: `400\tR-12/114 (60.0/40.0)` → ("R400", [("R12", 0.6), ("R114", 0.4)]), mass fractions. A
/// missing closing parenthesis is read as CoolProp's injection reads it (`removesuffix(')')`): row 401B lacks one.
pub fn parse_ashrae(text: &str) -> Result<Vec<AshraeRow>, Vec<String>> {
    let row = |line: &str| -> Option<AshraeRow> {
        let (number, blend) = line.split_once('\t')?;
        let (components, percents) = blend.strip_prefix("R-")?.rsplit_once('(')?;
        let percents = percents.trim();
        let percents = percents.strip_suffix(')').unwrap_or(percents);
        let percents: Vec<f64> = percents.split('/').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
        let names: Vec<String> = components.split('/').map(|c| format!("R{}", c.trim())).collect();
        let total: f64 = percents.iter().sum();
        (names.len() == percents.len() && total > 0.0).then(|| {
            (format!("R{}", number.trim()), names.into_iter().zip(percents.iter().map(|p| p / total)).collect())
        })
    };
    let lines = text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty());
    lines.map(|l| row(l).ok_or_else(|| vec![format!("{}: unreadable row {l:?}", FILES[0])])).collect()
}

/// The predefined mixtures: exact repeats dropped, differing repeats refused, fractions positive and summing to 1, and
/// ASHRAE 34's mass fractions attached where its list has the blend with the same components.
fn predefined_mixtures(
    entries: &[PredefinedJson],
    ashrae: &[AshraeRow],
    fluids: &[Source],
    notes: &mut Vec<String>,
) -> Result<Vec<Predefined>, Vec<String>> {
    let mut seen: BTreeMap<&str, &PredefinedJson> = BTreeMap::new();
    let mut errors = Vec::new();
    for entry in entries {
        match seen.get(entry.name.as_str()) {
            Some(first) if *first == entry => {
                notes.push(format!("predefined {}: an exact repeat, dropped", entry.name))
            }
            Some(_) => errors.push(format!("predefined {}: given twice, differently (ROT-041)", entry.name)),
            None => {
                seen.insert(&entry.name, entry);
            }
        }
    }
    for entry in seen.values() {
        let sum: f64 = entry.mole_fractions.iter().sum();
        let positive = entry.mole_fractions.iter().all(|x| x.is_finite() && *x > 0.0);
        if entry.fluids.len() != entry.mole_fractions.len() || !positive || (sum - 1.0).abs() > 1e-10 {
            errors.push(format!("predefined {}: fractions {:?} (sum {sum})", entry.name, entry.mole_fractions));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    for (name, row) in ashrae {
        let entry = seen.get(name.as_str());
        if entry.is_none_or(|entry| mass_fractions(entry, row, fluids).is_none()) {
            let components: Vec<&str> = row.iter().map(|(c, _)| c.as_str()).collect();
            let theirs = entry.map(|e| e.fluids.join("/")).unwrap_or_else(|| "no predefined mixture".into());
            notes.push(format!(
                "ASHRAE {name} ({}) does not match v8.0.0's {theirs}: its mass fractions are not kept",
                components.join("/")
            ));
        }
    }
    let predefined = seen.values().map(|entry| Predefined {
        name: entry.name.clone(),
        components: entry.fluids.clone(),
        mole_fractions: entry.mole_fractions.clone(),
        mass_fractions: ashrae
            .iter()
            .find(|(name, _)| *name == entry.name)
            .and_then(|(_, row)| mass_fractions(entry, row, fluids)),
    });
    Ok(predefined.collect())
}

/// The CAS number of the library fluid called `name` (its name or an alias, case-insensitively).
fn cas_of<'a>(fluids: &'a [Source], name: &str) -> Option<&'a str> {
    let called = |s: &&Source| {
        let info = &s.fluid.info;
        info.name.eq_ignore_ascii_case(name) || info.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    };
    fluids.iter().find(called).map(|s| s.fluid.info.cas.as_str())
}

/// An ASHRAE row's mass fractions in `entry`'s component order, where its components are `entry`'s: the same name or
/// the same library fluid (`R290` is `PROPANE`).
fn mass_fractions(entry: &PredefinedJson, row: &[(String, f64)], fluids: &[Source]) -> Option<Vec<f64>> {
    let same =
        |a: &str, b: &str| a.eq_ignore_ascii_case(b) || cas_of(fluids, a).is_some_and(|x| Some(x) == cas_of(fluids, b));
    let of = |fluid: &String| row.iter().find(|(c, _)| same(c, fluid)).map(|(_, w)| *w);
    (row.len() == entry.fluids.len()).then_some(())?;
    entry.fluids.iter().map(of).collect()
}

/// `x` as Rust source that parses back to its bits.
fn float(x: f64) -> String {
    let s = format!("{x:?}");
    if s.contains(['.', 'e', 'i', 'N']) { s } else { format!("{s}.0") }
}

fn floats(xs: &[f64]) -> String {
    xs.iter().map(|x| float(*x)).collect::<Vec<_>>().join(", ")
}

fn strings(xs: &[String]) -> String {
    xs.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>().join(", ")
}

/// `crates/phasekit-mix/src/generated.rs`.
pub fn generated_rs(library: &Library) -> String {
    let mut out = String::from(
        "// @generated by `cargo xtask datagen` from CoolProp 8.0.0's dev/mixtures (data/mixtures.lock) and dev/fluids\n\
         // (data/fluids.lock). Do not edit: PLAN.md M13.2.\n\n\
         use crate::data::{DepartureData, Kind, PairData, PredefinedData, Reducing, Term};\n\n",
    );
    let pairs = &library.pairs;
    let _ = writeln!(out, "/// v8.0.0's {} binary pairs, sorted by their CAS pair, β oriented to it.", pairs.len());
    let _ = writeln!(out, "pub(crate) static PAIRS: [PairData; {}] = [", pairs.len());
    for p in pairs {
        let reducing = match p.reducing {
            Reducing::Gerg(x) => format!("Reducing::Gerg([{}])", floats(&x)),
            Reducing::Lemmon(x) => format!("Reducing::Lemmon([{}])", floats(&x)),
        };
        let departure = p.departure.map_or("None".to_string(), |i| format!("Some({i})"));
        let _ = writeln!(
            out,
            "    PairData {{ cas: [{}], names: [{}], reducing: {reducing}, f: {}, departure: {departure}, source: {:?} }},",
            strings(&p.cas),
            strings(&p.names),
            float(p.f),
            p.source
        );
    }
    out.push_str("];\n\n");
    let departures = &library.departures;
    let _ = writeln!(out, "/// v8.0.0's {} departure functions; a pair names one by index.", departures.len());
    let _ = writeln!(out, "pub(crate) static DEPARTURES: [DepartureData; {}] = [", departures.len());
    for d in departures {
        let _ = writeln!(
            out,
            "    DepartureData {{ name: {:?}, aliases: &[{}], source: {:?}, kind: Kind::{:?}, power: {}, terms: &[",
            d.name,
            strings(&d.aliases),
            d.source,
            d.kind,
            d.power
        );
        for t in &d.terms {
            let _ = writeln!(
                out,
                "        Term {{ n: {}, t: {}, d: {}, l: {}, eta: {}, epsilon: {}, beta: {}, gamma: {} }},",
                float(t.n),
                float(t.t),
                float(t.d),
                float(t.l),
                float(t.eta),
                float(t.epsilon),
                float(t.beta),
                float(t.gamma)
            );
        }
        out.push_str("    ] },\n");
    }
    out.push_str("];\n\n");
    let predefined = &library.predefined;
    let _ = writeln!(
        out,
        "/// v8.0.0's {} predefined mixtures, by name; ASHRAE 34's mass fractions where its list has the blend.",
        predefined.len()
    );
    let _ = writeln!(out, "pub(crate) static PREDEFINED: [PredefinedData; {}] = [", predefined.len());
    for m in predefined {
        let mass = m.mass_fractions.as_ref().map_or("None".to_string(), |w| format!("Some(&[{}])", floats(w)));
        let _ = writeln!(
            out,
            "    PredefinedData {{ name: {:?}, components: &[{}], mole_fractions: &[{}], mass_fractions: {mass} }},",
            m.name,
            strings(&m.components),
            floats(&m.mole_fractions)
        );
    }
    out.push_str("];\n\n");
    let cas = &library.cas;
    let _ = writeln!(out, "/// Every v8.0.0 fluid's CAS number, by canonical name: a component's key into [`PAIRS`].");
    let _ = writeln!(out, "pub(crate) static CAS: [(&str, &str); {}] = [", cas.len());
    for (name, number) in cas {
        let _ = writeln!(out, "    ({name:?}, {number:?}),");
    }
    out.push_str("];\n");
    out
}

/// Every file the mixture step generates: (path, bytes).
pub fn outputs(library: &Library) -> Vec<(String, Vec<u8>)> {
    vec![(GENERATED.to_string(), generated_rs(library).into_bytes())]
}
