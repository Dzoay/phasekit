//! The `TRANSPORT` blocks as datagen reads them (PLAN.md M8.1; map 05 U2): each property's active entry into core's
//! closed forms, every key consumed or waived (R2), every length checked (R11), an empty list refused and the
//! alternates logged (R5).

use super::*;
use phasekit_core::internal::{ConductivityDilute, ConductivityModel, TransportRecord, ViscosityModel};
use serde_json::{Value, json};

/// The `TRANSPORT` block of a pinned file.
fn block(file: &str) -> Value {
    let value: Value = serde_json::from_str(&text(file)).unwrap();
    value["TRANSPORT"].clone()
}

/// The transport record of a pinned file's block after `edit`.
fn mapped(file: &str, edit: impl FnOnce(&mut Value)) -> Result<TransportRecord, String> {
    let mut b = block(file);
    edit(&mut b);
    transport::record(file, Some(&b))
}

/// The value at the JSON pointer `at`.
fn at<'v>(v: &'v mut Value, at: &str) -> &'v mut Value {
    v.pointer_mut(at).unwrap_or_else(|| panic!("no {at}"))
}

/// Map 05 R11: CoolProp checks a block's parallel lists with `assert`, which a release build drops, and reads the
/// friction-theory and ρ*_sr-CS arrays without a check (`TransportRoutines.cpp:367-384`). Datagen refuses unequal
/// lists and a fixed-size array of another size, naming the block.
#[test]
fn transport_lengths_are_checked() {
    let cases = [
        ("Nitrogen.json", "/viscosity/higher_order/t1", "TRANSPORT.viscosity.higher_order: t1 has 4 entries, a has 5"),
        ("Nitrogen.json", "/viscosity/higher_order/q", "TRANSPORT.viscosity.higher_order: q has 0 entries, p has 1"),
        ("Nitrogen.json", "/viscosity/dilute/t", "TRANSPORT.viscosity.dilute: t has 4 entries, a has 5"),
        (
            "n-Propane.json",
            "/viscosity/initial_density/t",
            "TRANSPORT.viscosity.initial_density: t has 8 entries, b has 9",
        ),
        (
            "Nitrogen.json",
            "/conductivity/residual/gamma",
            "TRANSPORT.conductivity.residual: gamma has 5 entries, A has 6",
        ),
        ("Methane.json", "/viscosity/higher_order/Aa", "TRANSPORT.viscosity.higher_order: Aa has 2 entries, not 3"),
        ("Methane.json", "/viscosity/higher_order/Ai", "TRANSPORT.viscosity.higher_order: Ai has 2 entries, not 3"),
        ("R1234yf.json", "/viscosity/0/c_liq", "TRANSPORT.viscosity[0]: c_liq has 3 entries, not 4"),
        ("R11.json", "/viscosity/psi/t", "TRANSPORT.viscosity.psi: t has 1 entries, a has 2"),
        ("R11.json", "/conductivity/f_int/t", "TRANSPORT.conductivity.f_int: t has 0 entries, a has 1"),
        ("n-Propane.json", "/conductivity/dilute/m", "TRANSPORT.conductivity.dilute: m has 0 entries, B has 1"),
        ("n-Propane.json", "/conductivity/residual/d", "TRANSPORT.conductivity.residual: d has 9 entries, B has 10"),
    ];
    for (file, list, want) in cases {
        assert!(mapped(file, |_| {}).is_ok(), "{file}");
        let err = mapped(file, |b| _ = at(b, list).as_array_mut().unwrap().pop()).unwrap_err();
        assert_eq!(err, want, "{file} {list}");
    }

    // Four blocks list one exponent more than they have coefficients: EthylBenzene's ECS ψ and the dilute terms of
    // R407C, R410A and R507A. CoolProp sums over the coefficients (`TransportRoutines.cpp:683, 1213`); datagen reads the
    // same terms, under a logged waiver for those blocks alone. A missing exponent there is still refused, and so is an
    // extra one anywhere else.
    let ViscosityModel::Ecs(ecs) = mapped("EthylBenzene.json", |_| {}).unwrap().viscosity.unwrap().model else {
        panic!("EthylBenzene's viscosity is ECS")
    };
    assert_eq!((&ecs.psi.a[..], &ecs.psi.t[..]), (&[0.846125588, 0.046896067][..], &[0.0, 1.0][..]));
    for blend in ["R407C", "R410A", "R507A"] {
        let file = format!("{blend}.json");
        let dilute = match mapped(&file, |_| {}).unwrap().conductivity.unwrap().model {
            ConductivityModel::Staged(staged) => staged.dilute,
            other => panic!("{blend}: {other:?}"),
        };
        let ConductivityDilute::RatioOfPolynomials { numerator, .. } = dilute else { panic!("{blend}: {dilute:?}") };
        assert_eq!((numerator.a.len(), &numerator.t[..]), (2, &[0.0, 1.0][..]), "{blend}");
    }
    let sources = sources();
    let waived: Vec<String> =
        sources.iter().flat_map(transport::skipped).filter(|l| !l.contains("alternate")).collect();
    let line = |file: &str, block: &str| {
        format!(
            "{file}: {block} lists more exponents than coefficients; the extra ones are dropped, as CoolProp drops them"
        )
    };
    assert_eq!(
        waived,
        [
            line("EthylBenzene.json", "TRANSPORT.viscosity.psi"),
            line("R407C.json", "TRANSPORT.conductivity.dilute"),
            line("R410A.json", "TRANSPORT.conductivity.dilute"),
            line("R507A.json", "TRANSPORT.conductivity.dilute"),
        ]
    );
    let short = mapped("EthylBenzene.json", |b| at(b, "/viscosity/psi/t").as_array_mut().unwrap().truncate(1));
    assert_eq!(short, Err("TRANSPORT.viscosity.psi: t has 1 entries, a has 2".into()));
    let extra = mapped("R11.json", |b| at(b, "/viscosity/psi/t").as_array_mut().unwrap().push(json!(2)));
    assert_eq!(extra, Err("TRANSPORT.viscosity.psi: t has 3 entries, a has 2".into()));
}

/// Friction theory's exponents are integers in `0..=MAX_POW`, and its optional coefficients come whole: `Aaaa`
/// without `Arrr` (CoolProp drops it silently), or `Nii` without `Aii`, is refused.
#[test]
fn friction_exponents_and_partners_are_checked() {
    let with =
        |key: &str, value: Value| mapped("SulfurHexafluoride.json", |b| b["viscosity"]["higher_order"][key] = value);
    assert!(with("Na", json!(16)).is_ok());
    for bad in [json!(17), json!(1.5), json!(-1)] {
        assert_eq!(with("Na", bad), Err("TRANSPORT.viscosity.higher_order: Na is not an integer in 0..=16".into()));
    }
    let without = |file: &str, key: &str| {
        mapped(file, |b| _ = b["viscosity"]["higher_order"].as_object_mut().unwrap().remove(key))
    };
    let alone = "TRANSPORT.viscosity.higher_order: unknown field";
    assert_eq!(without("SulfurHexafluoride.json", "Arrr"), Err(format!("{alone} \"Aaaa\"")));
    assert_eq!(without("SulfurHexafluoride.json", "Aaaa"), Err("TRANSPORT.viscosity.higher_order: no Aaaa".into()));
    assert_eq!(without("Methane.json", "Aii"), Err(format!("{alone} \"Nii\"")));
    assert_eq!(
        without("Methane.json", "Adrdr"),
        Err("TRANSPORT.viscosity.higher_order: friction theory takes exactly one of Arr and Adrdr".into())
    );
}

/// Every leaf of `v` as a JSON pointer: numbers, strings and lists of numbers; an object or a list of objects (the
/// model list) is walked.
fn leaves(v: &Value, path: String, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => map.iter().for_each(|(k, x)| leaves(x, format!("{path}/{k}"), out)),
        Value::Array(xs) if xs.iter().all(Value::is_object) && !xs.is_empty() => {
            xs.iter().enumerate().for_each(|(i, x)| leaves(x, format!("{path}/{i}"), out));
        }
        _ => out.push(path),
    }
}

/// A different, still well-typed value: an integer plus one, a float scaled, a string extended, a list's first
/// number changed.
fn perturb(v: &mut Value) {
    match v {
        Value::Number(n) if n.is_i64() || n.is_u64() => *v = json!(n.as_i64().unwrap_or_default() + 1),
        Value::Number(n) => *v = json!(n.as_f64().unwrap_or_default() * 1.5 + 0.25),
        Value::String(s) => s.push('~'),
        Value::Array(xs) => perturb(&mut xs[0]),
        other => panic!("unexpected leaf {other}"),
    }
}

/// Why a key need not reach the record: documentation (`_note`, `note`), or an alternate entry of a model list,
/// which datagen drops and logs (ROT-109).
fn waiver(path: &str) -> Option<&'static str> {
    let mut parts = path.split('/').skip(1);
    let entry = (parts.next(), parts.next().and_then(|i| i.parse::<usize>().ok()));
    if matches!(entry, (Some("viscosity"), Some(1..))) {
        return Some("alternate");
    }
    let key = path.rsplit('/').next().unwrap_or_default();
    (key == "_note" || key == "note").then_some("documentation")
}

/// Rot: ROT-110 (map 05 R2). Every key of every active `TRANSPORT` entry is consumed or waived: changing its value
/// changes the record, or datagen refuses the file (a form's `type` or `hardcoded` name, a `*_units` string it
/// checks), or the key is documentation or part of an alternate entry. CoolProp never reads the 18 ECS conductivity
/// blocks' `q_D` (`FluidLibrary.h:668-680`); datagen reads it.
#[test]
fn every_transport_key_is_consumed_or_waived() {
    let (mut consumed, mut checked, mut unread) = (0, Vec::new(), Vec::new());
    let mut waived: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut q_d = 0;
    for source in sources() {
        let Some(base) = source.fluid.transport.clone() else { continue };
        let record = transport::record(&source.file, Some(&base)).unwrap();
        let mut paths = Vec::new();
        leaves(&base, String::new(), &mut paths);
        for path in paths {
            if let Some(reason) = waiver(&path) {
                *waived.entry(reason).or_default() += 1;
                continue;
            }
            let mut changed = base.clone();
            perturb(at(&mut changed, &path));
            match transport::record(&source.file, Some(&changed)) {
                Ok(r) if r == record => unread.push(format!("{} {path}", source.file)),
                Ok(_) => consumed += 1,
                Err(_) => checked.push(path.rsplit('/').next().unwrap_or_default().to_string()),
            }
            q_d += usize::from(path == "/conductivity/q_D");
        }
    }
    assert!(unread.is_empty(), "keys no record reads: {unread:#?}");
    let checked_ok = |k: &String| k == "type" || k == "hardcoded" || k.ends_with("_units");
    assert!(checked.iter().all(checked_ok), "{checked:?}");
    assert_eq!(q_d, 18);
    assert_eq!(waived.get("documentation"), Some(&4));
    assert!(waived.get("alternate").is_some_and(|n| *n > 7 * 5), "{waived:?}");
    assert!(consumed > 1500, "{consumed}");
}

/// Rot: ROT-109. An empty model list is refused (CoolProp's `front()` on it was undefined behaviour, `FluidLibrary.h
/// :715-718`, fixed upstream in 538d4227), and so is a conductivity list, which v8.0.0 cannot parse (`:942-944`).
#[test]
fn an_empty_transport_list_is_a_load_error() {
    assert_eq!(
        mapped("R1234yf.json", |b| b["viscosity"] = json!([])),
        Err("TRANSPORT.viscosity is an empty list".into())
    );
    assert_eq!(
        mapped("Nitrogen.json", |b| b["viscosity"] = json!([])),
        Err("TRANSPORT.viscosity is an empty list".into())
    );
    let listed = mapped("Nitrogen.json", |b| b["conductivity"] = json!([b["conductivity"].clone()]));
    assert_eq!(listed, Err("TRANSPORT.conductivity is a list; v8.0.0 has one conductivity model per fluid".into()));
    assert_eq!(
        mapped("Nitrogen.json", |b| b["viscosity"] = json!(1.0)),
        Err("TRANSPORT.viscosity is not an object".into())
    );
}

/// Rot: ROT-109 (map 05 R5). Of a model list only the first entry is CoolProp's default: the record holds that
/// entry's form with that entry's own source (D14), and datagen logs each alternate it drops. Seven v8.0.0 files list
/// two viscosity models, each a ρ*_sr-CS model first.
#[test]
fn transport_source_is_the_active_entry() {
    let sources = sources();
    let listed: Vec<&Source> =
        sources.iter().filter(|s| s.fluid.transport.as_ref().is_some_and(|t| t["viscosity"].is_array())).collect();
    let names: Vec<&str> = listed.iter().map(|s| s.fluid.info.name.as_str()).collect();
    assert_eq!(names, ["R1234yf", "R1234ze(E)", "R124", "R152A", "R22", "R245fa", "R32"]);
    for source in listed {
        let block = source.fluid.transport.as_ref().unwrap();
        let entries = block["viscosity"].as_array().unwrap();
        let viscosity = transport::record(&source.file, Some(block)).unwrap().viscosity.unwrap();
        assert_eq!(&*viscosity.source.bibkey, entries[0]["BibTeX"].as_str().unwrap());
        assert!(matches!(viscosity.model, ViscosityModel::RhoSrCs(_)), "{}", source.file);
        let alternate = entries[1]["BibTeX"].as_str().unwrap();
        let line = format!("{}: TRANSPORT.viscosity[1] ({alternate}) is an alternate, not mapped", source.file);
        assert_eq!(transport::skipped(source), [line]);
    }
    // A single entry is the active one; the conductivity block has its own source.
    let nitrogen = block("Nitrogen.json");
    let record = transport::record("Nitrogen.json", Some(&nitrogen)).unwrap();
    assert_eq!(&*record.viscosity.unwrap().source.bibkey, nitrogen["viscosity"]["BibTeX"].as_str().unwrap());
    assert_eq!(&*record.conductivity.unwrap().source.bibkey, nitrogen["conductivity"]["BibTeX"].as_str().unwrap());
    // Reversed, R22's list makes its ECS entry the active one, with its own source.
    let swapped = mapped("R22.json", |b| b["viscosity"].as_array_mut().unwrap().reverse()).unwrap().viscosity.unwrap();
    assert!(matches!(swapped.model, ViscosityModel::Ecs(_)));
    assert_eq!(&*swapped.source.bibkey, "Klein-IJR-1997");
    assert!(matches!(mapped("R22.json", |_| {}).unwrap().conductivity.unwrap().model, ConductivityModel::Ecs(_)));
}

/// PLAN.md M8.4: the `ANCILLARIES.surface_tension` blocks are read as strictly as `TRANSPORT`: the source, the fit's own
/// `Tc`, equal-length `a` and `n`, and a `description` stating the form evaluated. Another formula, unequal lists and an
/// unknown key are refused. 108 of the 136 files have a fit.
#[test]
fn surface_tension_blocks_are_read_strictly() {
    let fit = |file: &str, edit: &dyn Fn(&mut Value)| {
        let mut value: Value = serde_json::from_str(&text(file)).unwrap();
        let mut block = value["ANCILLARIES"]["surface_tension"].take();
        edit(&mut block);
        transport::surface_tension(file, Some(&block))
    };
    let water = fit("Water.json", &|_| {}).unwrap().unwrap();
    assert_eq!((&*water.source.bibkey, water.t_c), ("Mulero-JPCRD-2012", 647.096));
    assert_eq!((&water.terms.a[..], &water.terms.t[..]), (&[-0.1306, 0.2151][..], &[2.471, 1.233][..]));
    let at = "ANCILLARIES.surface_tension";
    let formula = fit("Water.json", &|b| b["description"] = json!("sigma = a*T")).unwrap_err();
    assert_eq!(formula, format!("{at}: description \"sigma = a*T\", not \"sigma = sum(a_i*(1-T/Tc)^n_i)\""));
    let short = fit("Water.json", &|b| _ = b["n"].as_array_mut().unwrap().pop()).unwrap_err();
    assert_eq!(short, format!("{at}: n has 1 entries, a has 2"));
    let unknown = fit("Water.json", &|b| b["Tmin"] = json!(273.16)).unwrap_err();
    assert_eq!(unknown, format!("{at}: unknown field \"Tmin\""));
    assert_eq!(transport::surface_tension("x.json", None), Ok(None));
    let fits = sources().iter().filter(|s| s.fluid.ancillaries.get("surface_tension").is_some()).count();
    assert_eq!(fits, 108);
}
