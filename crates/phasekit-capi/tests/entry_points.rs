//! Rot: ROT-150 (map 12 §9; map 11 F2, F16). The C entry points over arbitrary input: each string argument any bytes
//! with their own length, a null pointer, or a length over PTRDIFF_MAX; never a length past a real buffer, which the C
//! contract makes the caller's to keep. Fluid strings include mixture names well formed and not, from the grammar's
//! edges (PLAN.md M13.14). No call panics, even into its catch (`PK_PANIC` is no status the test accepts), or writes
//! its output on failure, and each status is one the ABI defines; `pk_batch` gets any point and cell counts its buffers
//! hold, and writes its cells only on success. Every `pk_*` export is called, and every function of the Tier A shim,
//! whose `errstring` never reports a caught panic. The seed is fixed, so every machine runs the same cases.

#![cfg(not(feature = "coolproplib-shim"))] // the shim's build exports no pk_* function
#![allow(unsafe_code, reason = "the test calls the unsafe exports, each with valid buffers")]
#![allow(clippy::unwrap_used, reason = "test-crate helpers")]

use std::ffi::{CString, c_char, c_int};

use phasekit_capi::*;
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed, TestCaseError};

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 1024,
        rng_seed: RngSeed::Fixed(0x706b_5f63_6170_6921),
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct("proptest-regressions/entry_points.txt"))),
        ..ProptestConfig::default()
    }
}

/// One string argument as a C caller may pass it.
#[derive(Clone, Debug)]
enum Arg {
    Bytes(Vec<u8>),
    Null,
    Huge,
}

impl Arg {
    /// Its pointer and length.
    fn raw(&self) -> (*const c_char, usize) {
        match self {
            Arg::Bytes(b) => (b.as_ptr().cast(), b.len()),
            Arg::Null => (std::ptr::null(), 3),
            Arg::Huge => (b"T".as_ptr().cast(), usize::MAX),
        }
    }
}

/// Keys and fluid strings the grammar knows, mixture names, junk bytes, nulls and huge lengths.
fn arg() -> impl Strategy<Value = Arg> {
    let known = ["T", "P", "Q", "D", "Hmass", "Water", "HEOS::R134a", "d(P)/d(T)|Dmolar", "Phase", "Tcrit", "IIR"];
    prop_oneof![
        6 => prop::sample::select(known.to_vec()).prop_map(|k| Arg::Bytes(k.as_bytes().to_vec())),
        3 => mixture().prop_map(|m| Arg::Bytes(m.into_bytes())),
        3 => prop::collection::vec(any::<u8>(), 0..24).prop_map(Arg::Bytes),
        1 => Just(Arg::Null),
        1 => Just(Arg::Huge),
    ]
}

/// A mixture name, well formed or nearly: a prefix, then up to three parts joined by `&` (or `&&`), each a name and a
/// fraction in one of the grammar's forms (its brackets whole, missing, doubled or padded), or a `.mix` name.
fn mixture() -> impl Strategy<Value = String> {
    let prefix = prop::sample::select(vec!["", "HEOS::", "GERG2008::", "HEOS::GERG2008::"]);
    let names = ["R32", "R125", "Water", "Methane", "Ethane", "Ammonia", "", "R3]2", "HEOS::R125", "R410A.mix"];
    let fractions =
        ["0.5", "0", "1", "1.0", "0,5", " 0.5", "nan", "inf", "-0.5", "1.5", "1e-20", ".5", "5e-1", "0x1", ""];
    let forms = ["{n}[{x}]", "{n}[{x}", "{n}{x}]", "{n}[[{x}]]", "{n}", "{n}[{x}] "];
    let part = (
        prop::sample::select(names.to_vec()),
        prop::sample::select(fractions.to_vec()),
        prop::sample::select(forms.to_vec()),
    )
        .prop_map(|(n, x, form)| form.replace("{n}", n).replace("{x}", x));
    let composed = (prefix, prop::collection::vec(part, 0..=3), prop::sample::select(vec!["&", "&&"]))
        .prop_map(|(prefix, parts, sep)| format!("{prefix}{}", parts.join(sep)));
    let named = ["R410A.mix", "HEOS::r407c.MIX", "Air.mix", "R999.mix", ".mix", "R410A.mi", "GERG2008::R410A.mix"];
    // Names whose last four bytes split a character, and non-ASCII names and fractions.
    let named = named.into_iter().chain(["\u{e9}aaa", "HEOS::\u{e9}aaa", "R\u{e9}.mix", "R32[0\u{b7}5]&R125[0.5]"]);
    prop_oneof![4 => composed, 1 => prop::sample::select(named.collect::<Vec<_>>()).prop_map(String::from)]
}

/// Every status the ABI defines.
const STATUSES: [i32; 17] = [
    PK_OK,
    PK_INVALID_INPUT,
    PK_UNSUPPORTED,
    PK_DOMAIN,
    PK_NO_CONVERGENCE,
    PK_AMBIGUOUS,
    PK_UNDEFINED,
    PK_NO_MODEL,
    PK_LOAD,
    PK_OTHER,
    PK_UNKNOWN_KEY,
    PK_NULL_POINTER,
    PK_BAD_LENGTH,
    PK_BAD_UTF8,
    PK_STALE_HANDLE,
    PK_BUFFER_TOO_SMALL,
    PK_EXTRAPOLATED,
];

fn registry() -> pk_registry {
    let mut handle = 0;
    // SAFETY: `handle` is writable.
    assert_eq!(unsafe { pk_registry_new(PK_DATASET_CORRECTED, &mut handle) }, PK_OK);
    handle
}

fn water(registry: pk_registry) -> pk_fluid {
    let mut handle = 0;
    // SAFETY: "Water" is 5 bytes; `handle` is writable.
    assert_eq!(unsafe { pk_fluid_new(registry, c"Water".as_ptr(), 5, &mut handle) }, PK_OK);
    handle
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn entry_points_take_any_bytes_and_lengths(
        args in prop::collection::vec(arg(), 4),
        values in prop::collection::vec(prop_oneof![Just(300.0), Just(1e5), Just(0.5), any::<f64>()], 2),
        len in 0usize..12,
        (n, cells) in (0usize..=2, 0usize..=4),
        (data_set, stale) in (any::<i32>(), any::<u64>()),
    ) {
        let registry = registry();
        let [o, a, b, f] = [args[0].raw(), args[1].raw(), args[2].raw(), args[3].raw()];
        let mut value = -7.0;
        // SAFETY: each pointer is null or its own length's bytes, or a length over PTRDIFF_MAX; `value` is writable.
        let status = unsafe { pk_props_si(registry, o.0, o.1, a.0, a.1, values[0], b.0, b.1, values[1], f.0, f.1, &mut value) };
        prop_assert!(STATUSES.contains(&status), "{status}");
        prop_assert!(status == PK_OK || value == -7.0, "{status} wrote {value}");
        let (mut buf, mut needed) = ([b'#' as c_char; 12], 0);
        // SAFETY: as above; `buf` is 12 writable bytes and `len` ≤ 12.
        let status = unsafe { pk_phase_si(registry, a.0, a.1, values[0], b.0, b.1, values[1], f.0, f.1, buf.as_mut_ptr(), len, &mut needed) };
        prop_assert!(STATUSES.contains(&status), "{status}");
        prop_assert!(buf[len..].iter().all(|&c| c == b'#' as c_char), "wrote past {len}");
        let mut fluid = 0;
        // SAFETY: as above; `fluid` is writable.
        let status = unsafe { pk_fluid_new(registry, f.0, f.1, &mut fluid) };
        prop_assert!(STATUSES.contains(&status) && (status == PK_OK) == (fluid != 0), "{status} {fluid}");
        if fluid != 0 {
            let (mut name, mut needed) = ([b'#' as c_char; 12], 0);
            // SAFETY: `name` is 12 writable bytes and `len` ≤ 12; `needed` is writable.
            let status = unsafe { pk_fluid_name(fluid, name.as_mut_ptr(), len, &mut needed) };
            prop_assert!(status == PK_OK || status == PK_BUFFER_TOO_SMALL, "{status}");
            prop_assert!(name[len..].iter().all(|&c| c == b'#' as c_char), "wrote past {len}");
        }
        prop_assert!(fluid == 0 || pk_fluid_free(fluid) == PK_OK);
        let mut one = -7.0;
        // SAFETY: as above; `one` is writable.
        let status = unsafe { pk_props1_si(registry, o.0, o.1, f.0, f.1, &mut one) };
        prop_assert!(STATUSES.contains(&status) && (status == PK_OK || one == -7.0), "{status} wrote {one}");
        let mut air = 0;
        // SAFETY: `air` is writable.
        prop_assert_eq!(unsafe { pk_humid_air_new(registry, &mut air) }, PK_OK);
        let mut ha = -7.0;
        // SAFETY: as above; `ha` is writable.
        let status = unsafe { pk_ha_props_si(air, o.0, o.1, a.0, a.1, values[0], b.0, b.1, values[1], f.0, f.1, values[1], &mut ha) };
        prop_assert!(STATUSES.contains(&status) && (status == PK_OK || ha == -7.0), "{status} wrote {ha}");
        prop_assert_eq!(pk_humid_air_free(air), PK_OK);
        prop_assert!(STATUSES.contains(&pk_humid_air_free(stale)));
        let mut gauged = 0;
        // SAFETY: as above; `gauged` is writable.
        let status = unsafe { pk_registry_with_reference(registry, f.0, f.1, a.0, a.1, &mut gauged) };
        prop_assert!(STATUSES.contains(&status) && (status == PK_OK) == (gauged != 0), "{status} {gauged}");
        prop_assert!(gauged == 0 || pk_registry_free(gauged) == PK_OK);
        let mut other = 0;
        // SAFETY: `other` is writable.
        let status = unsafe { pk_registry_new(data_set, &mut other) };
        prop_assert!(STATUSES.contains(&status) && (status == PK_OK) == (other != 0), "{data_set}: {status}");
        prop_assert!(other == 0 || pk_registry_free(other) == PK_OK);
        prop_assert!(STATUSES.contains(&pk_fluid_free(stale)) && STATUSES.contains(&pk_registry_free(stale)));
        let mut last = [b'#' as c_char; 12];
        // SAFETY: `last` is 12 writable bytes and `len` ≤ 12.
        let needed = unsafe { pk_last_error(last.as_mut_ptr(), len) };
        prop_assert!(needed >= 1 && last[len..].iter().all(|&c| c == b'#' as c_char), "{needed} wrote past {len}");
        let water = water(registry);
        let ((keys, lens), (mut out, mut codes)) = (([o.0], [o.1]), ([-7.0; 4], [-9; 4]));
        // SAFETY: as above; `values` holds 2 doubles and `n` ≤ 2; `out` and `codes` hold 4 cells and `cells` ≤ 4.
        let status = unsafe { pk_batch(water, keys.as_ptr(), lens.as_ptr(), 1, a.0, a.1, values.as_ptr(), b.0, b.1, values.as_ptr(), n, out.as_mut_ptr(), codes.as_mut_ptr(), cells) };
        prop_assert!(STATUSES.contains(&status), "{status}");
        let written = if status == PK_OK { cells } else { 0 };
        prop_assert!(codes[..written].iter().all(|c| STATUSES.contains(c)), "{codes:?}");
        prop_assert!(out[written..].iter().all(|&v| v == -7.0) && codes[written..].iter().all(|&c| c == -9), "{status} wrote {out:?}");
        prop_assert_eq!(pk_fluid_free(water), PK_OK);
        prop_assert_eq!(pk_registry_free(registry), PK_OK);
    }
}

/// One C string argument: a key or fluid string the shim knows, a mixture name, any bytes but NUL, or a null pointer.
fn c_arg() -> impl Strategy<Value = Option<CString>> {
    let known =
        ["T", "P", "Q", "Dmolar", "Hmass", "Water", "R410A.mix", "errstring", "FluidsList", "predefined_mixtures"];
    let known = known.into_iter().chain(["name", "aliases", "PT_INPUTS", "Phase", "FLOAT_PUNCTUATION", "R_U_CODATA"]);
    let text = |s: String| CString::new(s).ok();
    prop_oneof![
        5 => prop::sample::select(known.collect::<Vec<_>>()).prop_map(move |k| text(k.to_string())),
        3 => mixture().prop_map(text),
        3 => prop::collection::vec(1u8..=255, 0..24).prop_map(|b| CString::new(b).ok()),
        1 => Just(None),
    ]
}

/// An argument's pointer: its string's, or null.
fn ptr(arg: &Option<CString>) -> *const c_char {
    arg.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
}

/// The shim's `errstring` (read, which clears it) names no caught panic.
fn no_caught_panic(call: &str) -> Result<(), TestCaseError> {
    let (key, mut buf) = (c"errstring", [0 as c_char; 512]);
    // SAFETY: `key` is NUL-terminated; `buf` holds 512 bytes.
    unsafe { shim::get_global_param_string(key.as_ptr(), buf.as_mut_ptr(), 512) };
    let message: String = buf.iter().take_while(|&&c| c != 0).map(|&c| c as u8 as char).collect();
    prop_assert!(!message.contains("panic"), "{call}: {message}");
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn shim_entry_points_take_any_strings(
        args in prop::collection::vec(c_arg(), 4),
        values in prop::collection::vec(prop_oneof![Just(300.0), Just(1e5), Just(0.5), any::<f64>()], 2),
        (n, flag) in (0usize..=16, any::<bool>()),
    ) {
        let [o, a, b, f] = [ptr(&args[0]), ptr(&args[1]), ptr(&args[2]), ptr(&args[3])];
        let (mut buf, size) = ([b'#' as c_char; 16], c_int::try_from(n).unwrap());
        let untouched = |buf: &[c_char; 16]| buf[n..].iter().all(|&c| c == b'#' as c_char);
        // SAFETY (every call below): each string is null or NUL-terminated; `buf` holds 16 bytes and `n` ≤ 16.
        unsafe { shim::PropsSI(o, a, values[0], b, values[1], f) };
        no_caught_panic("PropsSI")?;
        unsafe { shim::Props1SI(f, o) };
        no_caught_panic("Props1SI")?;
        unsafe { shim::PhaseSI(a, values[0], b, values[1], f, buf.as_mut_ptr(), size) };
        prop_assert!(untouched(&buf), "PhaseSI wrote past {n}");
        no_caught_panic("PhaseSI")?;
        unsafe { shim::HAPropsSI(o, a, values[0], b, values[1], f, values[1]) };
        no_caught_panic("HAPropsSI")?;
        unsafe { shim::get_global_param_string(o, buf.as_mut_ptr(), size) };
        prop_assert!(untouched(&buf), "get_global_param_string wrote past {n}");
        no_caught_panic("get_global_param_string")?;
        unsafe { shim::get_fluid_param_string(f, o, buf.as_mut_ptr(), size) };
        prop_assert!(untouched(&buf), "get_fluid_param_string wrote past {n}");
        no_caught_panic("get_fluid_param_string")?;
        unsafe { (shim::get_param_index(o), shim::get_input_pair_index(a)) };
        no_caught_panic("get_param_index")?;
        unsafe { shim::set_config_string(a, b) };
        unsafe { shim::set_config_bool(a, flag) };
        unsafe { shim::set_config_double(a, values[0]) };
        no_caught_panic("set_config_*")?;
    }
}
