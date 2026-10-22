//! Mixture names (PLAN.md M13.13): CoolProp's `A[x]&B[y]` strings and its predefined `.mix` names through compat, as
//! v8.0.0's `PropsSI` reads them; the pseudo-pure names unchanged beside them; and one strict grammar (ROT-166).
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_compat::{CompatError, props_si_in, props1_si_in};
use phasekit_core::{DataSet, Error, LoadError, Registry};
use phasekit_verify::{CRITICAL_NOT_REDUCING, DIVERGENCES, Fixture, Tolerance, ToleranceClass, fixture};

/// The embedded registry on `set`, with mixture names.
fn names(set: DataSet) -> Registry {
    phasekit_mix::with_mixtures(&Registry::from_embedded(set).unwrap(), set)
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/facts/mixtures.csv. v8.0.0's `PropsSI` and `Props1SI` on
/// `HEOS::R32[0.697615]&R125[0.302385]`, the same beside R134a at 0, `R32[1.0]&R125[0.0]` (R32 alone), `R410A.mix`,
/// `HEOS::R410A.mix`, `R407C.mix` and CO₂/N₂ at 50/50, read on `Parity`: the molar mass within 4 ulp (Σ x_i M_i summed
/// as v8.0.0 sums it or not), the density and enthalpy at 300 K and 1 bar within `Flash`, and the bubble and dew
/// pressures at 250 K within DIV-0060's measured bound (v8.0.0's Newton stops on its step). Not asserted: R407C's
/// enthalpy, whose R134a is one of DIV-0053's fluids (v8.0.0 takes its α⁰ at its critical T in a mixture).
#[test]
fn compat_reads_coolprop_mixture_strings() {
    let (path, text) = fixture!("coolprop-8.0.0/facts/mixtures.csv");
    let facts = Fixture::parse(path, text).unwrap();
    let registry = names(DataSet::Parity);
    let entry = DIVERGENCES.iter().find(|d| d.id == "DIV-0060").unwrap();
    let Some(Tolerance::Relative(saturation)) = entry.tolerance else { panic!("DIV-0060 has no relative bound") };
    let (mut misses, mut skipped, mut compared) = (Vec::new(), 0, 0);
    for row in 0..facts.rows().len() {
        let (name, call) = (facts.printed(row, "name").unwrap(), facts.printed(row, "call").unwrap());
        let want = facts.value(row, "value").unwrap();
        let (function, args) = call.strip_suffix(')').and_then(|c| c.split_once('(')).unwrap();
        let args: Vec<&str> = args.split(';').collect();
        if function.starts_with("GlobalParam") {
            continue;
        }
        compared += 1;
        if args[0] == "Hmolar" && with_critical(&registry, args[5]) {
            skipped += 1;
            continue;
        }
        let number = |i: usize| args[i].parse::<f64>().unwrap();
        let (got, bound) = match function {
            "Props1SI" => (props1_si_in(&registry, args[1], args[0]), 4.0 * f64::EPSILON * want),
            _ => {
                let got = props_si_in(&registry, args[0], args[1], number(2), args[3], number(4), args[5]);
                let bound = if args[3] == "Q" { saturation * want } else { ToleranceClass::Flash.bound(want).unwrap() };
                (got, bound)
            }
        };
        match got {
            Ok(value) if (value - want).abs() <= bound.abs() => {}
            other => misses.push(format!("{name}: {other:?} against {want}")),
        }
    }
    assert!(misses.is_empty() && (compared, skipped) == (31, 1), "{compared} rows: {misses:#?}");
}

/// Whether mixture name `fluid` names one of DIV-0053's fluids.
fn with_critical(registry: &Registry, fluid: &str) -> bool {
    let resolved = registry.resolve(fluid).unwrap();
    let mixture = (resolved.model() as &dyn std::any::Any).downcast_ref::<phasekit_mix::Mixture>();
    mixture.is_some_and(|m| m.model().fluids().any(|f| CRITICAL_NOT_REDUCING.contains(&f.info().name())))
}

/// User decision 4: the pseudo-pure blends keep their names and behaviour beside the true mixtures. Through a registry
/// with mixture names, `R410A`, `R407C`, `R404A`, `R507A`, `SES36` and `Air` give the plain registry's values bit for
/// bit, at a gas state and at Q = 0 and 1 (their ancillary definitions), and the fluids list is the plain one; `R410A.mix`
/// is the mixture of R32 and R125, whose dew pressure is its own.
#[test]
fn pseudo_pure_names_keep_their_behaviour() {
    let plain = Registry::from_embedded(DataSet::Corrected).unwrap();
    let mixed = names(DataSet::Corrected);
    let states = [("T", 300.0, "P", 1e5), ("T", 250.0, "Q", 0.0), ("T", 250.0, "Q", 1.0), ("T", 80.0, "Q", 0.0)];
    for fluid in ["R410A", "R407C", "R404A", "R507A", "SES36", "Air"] {
        for (k1, v1, k2, v2) in states {
            let at = |registry| props_si_in(registry, "Dmolar", k1, v1, k2, v2, fluid);
            assert_eq!(at(&mixed), at(&plain), "{fluid} at {k1} = {v1}, {k2} = {v2}");
        }
    }
    assert!(plain.names().eq(mixed.names()));
    let dew = |fluid| props_si_in(&mixed, "P", "T", 250.0, "Q", 1.0, fluid).unwrap();
    let mixture = phasekit_mix::predefined(&plain, "R410A", DataSet::Corrected).unwrap();
    let own = props_si_in(&mixed, "molarmass", "T", 300.0, "P", 1e5, "R410A.mix").unwrap();
    assert!(dew("R410A.mix") != dew("R410A") && own == mixture.info().molar_mass(), "{}", dew("R410A.mix"));
}

/// Rot: ROT-166 (map 01 R17). v8.0.0 reads a mixture string leniently: `R32[ 0.5]` is 0.5, a fraction under 1e-15 is
/// dropped, fractions that sum to 0.9 are taken as given, and its decimal point is a setting (`FLOAT_PUNCTUATION`).
/// Here one grammar, `[HEOS:: | GERG2008::] name[x]&name[x]...` or `[HEOS::] name.mix`, each x a plain decimal: a
/// string outside it is refused as such (`LoadError::Format`, naming the string), fractions outside [0, 1] or not
/// summing to 1 are M13.4's `InvalidInput`, a component that is not a fluid is `UnknownName`, and backend options,
/// concentrations and padding are compat's own refusal, before any lookup.
#[test]
fn malformed_mixture_strings_are_refused() {
    let registry = names(DataSet::Corrected);
    let call = |fluid: &str| props_si_in(&registry, "Dmolar", "T", 300.0, "P", 1e5, fluid);
    let malformed = [
        "R32&R125",
        "R32[0.5]&R125",
        "R32[0,5]&R125[0,5]",
        "R32[ 0.5]&R125[0.5]",
        "R32[0.5 ]&R125[0.5]",
        "R32[0.5] &R125[0.5]",
        "R32[0.5]&R125[0.5]&",
        "&R32[0.5]&R125[0.5]",
        "R32[0.5]&&R125[0.5]",
        "R32[0.5]R125[0.5]",
        "R32[[0.5]]&R125[0.5]",
        "R32[0.5]]&R125[0.5]",
        "[0.5]&R125[0.5]",
        "R32[]&R125[1.0]",
        "R32[nan]&R125[0.5]",
        "R32[inf]&R125[0.5]",
        "R32[0x1]&R125[0.5]",
        "R32[1/2]&R125[0.5]",
        "R32[0.5]&HEOS::R125[0.5]",
        "HEOS::GERG2008::Methane[0.5]&Ethane[0.5]",
        "R410A.mix&R32[0.5]",
        "GERG2008::",
    ];
    for fluid in malformed {
        let format =
            matches!(call(fluid), Err(CompatError::Core(Error::Load(LoadError::Format(ref m)))) if m.contains(fluid));
        assert!(format, "{fluid}: {:?}", call(fluid));
    }
    let invalid = |quantity, value| Err(CompatError::Core(Error::InvalidInput { quantity, value }));
    assert_eq!(call("R32[0.7]&R125[0.2]"), invalid(phasekit_mix::quantities::MOLE_FRACTION_SUM, 0.7 + 0.2));
    assert_eq!(call("R32[1.5]&R125[-0.5]"), invalid(phasekit_core::Quantity::MOLE_FRACTION, 1.5));
    let unknown = |name: &str| Err(CompatError::Core(Error::Load(LoadError::UnknownName(name.into()))));
    assert_eq!(call("R32[0.5]&Unobtainium[0.5]"), unknown("Unobtainium"));
    assert_eq!(call("R999.mix"), unknown("R999.mix"));
    for fluid in ["R32[0.5]&R125[0.5]?{}", "R32[0.5]&R125[0.5]%", " R32[0.5]&R125[0.5]", "R32[0.5]&R125[0.5] "] {
        assert_eq!(call(fluid), Err(CompatError::InvalidFluid(fluid.into())), "{fluid:?}");
    }
    let accepted = ["R32[0.5]&R125[0.5]", "HEOS::R32[.5]&R125[5e-1]", "R125[0.5]&R32[0.5]", "r410a.mix", "Water[1.0]"];
    for fluid in accepted.into_iter().chain(["GERG2008::Methane[0.9]&Ethane[0.1]", "GERG2008::Methane"]) {
        assert!(call(fluid).is_ok(), "{fluid}: {:?}", call(fluid));
    }
}

/// Oracle: CoolProp 8.0.0, fixtures/coolprop-8.0.0/facts/mixtures.csv (PLAN.md M13.14; user decision MX5). The Tier A
/// shim's `get_global_param_string("predefined_mixtures")` and `("mixture_binary_pairs_list")` are v8.0.0's strings:
/// the same items in the same order (each predefined mixture as `Name.mix` and as `NAME.MIX`, 294 in byte order; the
/// 888 pairs' sorted CAS numbers joined by `&`), by their count and the 52-bit sha256 digest of the whole string.
#[test]
fn shim_global_strings_list_v8_mixtures() {
    let (path, text) = fixture!("coolprop-8.0.0/facts/mixtures.csv");
    let facts = Fixture::parse(path, text).unwrap();
    let fact = |name: &str| {
        let row = (0..facts.rows().len()).find(|&r| facts.printed(r, "name") == Some(name)).unwrap();
        facts.value(row, "value").unwrap()
    };
    for param in ["predefined_mixtures", "mixture_binary_pairs_list"] {
        let list = phasekit_capi::shim::global_param(param).unwrap();
        let digest = u64::from_str_radix(&phasekit_verify::sha256_hex(list.as_bytes())[..13], 16).unwrap() as f64;
        let ours = (list.split(',').count() as f64, digest);
        assert_eq!(ours, (fact(&format!("{param}_items")), fact(&format!("{param}_digest"))), "{param}: {list:.200}");
    }
}
