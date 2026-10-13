//! The Tier A shim's v8.0.0 codes against the oracle (PLAN.md M10.6; D11; ROT-146, ROT-147; map 01 R1, R2;
//! VERIFICATION.md §3.5 `codes`): its parameter and input-pair codes, compat's phase codes and its configuration keys,
//! each exactly what CoolProp 8.0.0 has.

use phasekit_capi::shim::{Kind, config_keys, input_pair_code, parameter_code, parameter_names};
use phasekit_compat::{output_names, phase_index};
use phasekit_core::Phase;
use phasekit_verify::{Fixture, fixture};

/// The (name, code) rows of one table of the wheel's codes.
fn table<'a>(wheel: &'a Fixture<'a>, name: &'a str) -> impl Iterator<Item = (&'a str, f64)> + 'a {
    (0..wheel.rows().len()).filter(move |&i| wheel.printed(i, "table") == Some(name)).filter_map(move |i| {
        let code = wheel.value(i, "index")?;
        Some((wheel.printed(i, "name")?, code))
    })
}

/// The keys of the `config:` header's flat JSON object with their values' types: `true` or `false`, a string, a number
/// with a point or an exponent, an integer.
fn config_types(json: &str) -> Option<Vec<(&str, Kind)>> {
    let (mut rest, mut keys) = (json.strip_prefix('{')?, Vec::new());
    loop {
        let (key, value) = rest.strip_prefix('"')?.split_once("\":")?;
        let (kind, after) = match value.strip_prefix('"') {
            Some(text) => (Kind::Text, text.split_once('"')?.1),
            None => {
                let (scalar, after) = value.split_at(value.find([',', '}'])?);
                let kind = match scalar {
                    "true" | "false" => Kind::Bool,
                    _ if scalar.contains(['.', 'e', 'E']) => Kind::Double,
                    _ => Kind::Int,
                };
                (kind, after)
            }
        };
        keys.push((key, kind));
        match after.strip_prefix(',') {
            Some(next) => rest = next,
            None => return (after == "}").then_some(keys),
        }
    }
}

/// ROT-146, ROT-147 (map 01 R1, R2). Every parameter name `get_parameter_index` takes, and a few it refuses, has the
/// shim's code (-1 for a refusal); every phase code is compat's `phase_index`; every input-pair name has the code of
/// the first `input_pair_list` entry that bears it, and an enum name that is no pair name is refused, as v8.0.0's
/// `get_input_pair_index` does. The shim knows exactly v8.0.0's 38 configuration keys, each with its type.
#[test]
fn parameter_and_pair_codes_match_v8_0_0() {
    let (name, text) = fixture!("coolprop-8.0.0/codes/coolprop.csv");
    let wheel = Fixture::parse(name, text).unwrap();
    let (name, text) = fixture!("coolprop-8.0.0/codes/pair-names.csv");
    let names = Fixture::parse(name, text).unwrap();
    let mut failures = Vec::new();
    for (name, code) in table(&wheel, "parameter") {
        let got = parameter_code(name).map_or(-1.0, f64::from);
        if got != code {
            failures.push(format!("parameter `{name}`: {got}, v8.0.0's {code}"));
        }
    }
    let phases = [
        (Phase::Liquid, "phase_liquid"),
        (Phase::Supercritical, "phase_supercritical"),
        (Phase::SupercriticalGas, "phase_supercritical_gas"),
        (Phase::SupercriticalLiquid, "phase_supercritical_liquid"),
        (Phase::CriticalPoint, "phase_critical_point"),
        (Phase::Gas, "phase_gas"),
        (Phase::TwoPhase, "phase_twophase"),
        (Phase::Solid, "phase_unknown"),
    ];
    for (phase, name) in phases {
        let want = table(&wheel, "phase").find(|(n, _)| *n == name).map(|(_, code)| code);
        if want != Some(f64::from(phase_index(phase))) {
            failures.push(format!("{phase:?}: {}, v8.0.0's {name} {want:?}", phase_index(phase)));
        }
    }
    let pairs: Vec<(&str, f64)> = table(&wheel, "pair").collect();
    let mut first: Vec<(&str, f64)> = Vec::new();
    for row in 0..names.rows().len() {
        let (Some(name), Some(pair)) = (names.printed(row, "name"), names.printed(row, "pair")) else { continue };
        let code = pairs.iter().find(|(p, _)| *p == pair).map(|(_, code)| *code);
        match code {
            Some(code) if !first.iter().any(|(n, _)| *n == name) => first.push((name, code)),
            Some(_) => {}
            None => failures.push(format!("pair-names.csv: `{pair}` is not one of the wheel's pairs")),
        }
    }
    for (name, code) in &first {
        if input_pair_code(name).map(f64::from) != Some(*code) {
            failures.push(format!("pair `{name}`: {:?}, v8.0.0's {code}", input_pair_code(name)));
        }
    }
    for (pair, _) in &pairs {
        if !first.iter().any(|(n, _)| n == pair) && input_pair_code(pair).is_some() {
            failures.push(format!("`{pair}` is no pair name, but the shim takes it"));
        }
    }
    let config = wheel.header("config").and_then(config_types).unwrap_or_default();
    assert_eq!(config_keys().collect::<Vec<_>>(), config, "the configuration keys and their types");
    assert!(parameter_names().eq(output_names()), "the codes follow compat's output table");
    assert!(first.len() > 30 && table(&wheel, "parameter").count() > 190, "{} pair names", first.len());
    assert_eq!(failures, Vec::<String>::new());
}
