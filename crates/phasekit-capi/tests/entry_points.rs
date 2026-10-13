//! Rot: ROT-150 (map 12 §9; map 11 F2, F16). The C entry points over arbitrary input: each string argument any bytes
//! with their own length, a null pointer, or a length over PTRDIFF_MAX; never a length past a real buffer, which the C
//! contract makes the caller's to keep. No call panics or writes its output on failure, and each status is one the
//! ABI defines; `pk_batch` gets any point and cell counts its buffers hold, and writes its cells only on success. The seed is fixed, so every machine runs the same cases.

#![cfg(not(feature = "coolproplib-shim"))] // the shim's build exports no pk_* function
#![allow(unsafe_code, reason = "the test calls the unsafe exports, each with valid buffers")]
#![allow(clippy::unwrap_used, reason = "test-crate helpers")]

use std::ffi::c_char;

use phasekit_capi::*;
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, RngSeed};

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

/// Keys and fluid strings the grammar knows, junk bytes, nulls and huge lengths.
fn arg() -> impl Strategy<Value = Arg> {
    let known = ["T", "P", "Q", "D", "Hmass", "Water", "HEOS::R134a", "d(P)/d(T)|Dmolar", "Phase", "Tcrit", "IIR"];
    prop_oneof![
        6 => prop::sample::select(known.to_vec()).prop_map(|k| Arg::Bytes(k.as_bytes().to_vec())),
        3 => prop::collection::vec(any::<u8>(), 0..24).prop_map(Arg::Bytes),
        1 => Just(Arg::Null),
        1 => Just(Arg::Huge),
    ]
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
        prop_assert!(fluid == 0 || pk_fluid_free(fluid) == PK_OK);
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
