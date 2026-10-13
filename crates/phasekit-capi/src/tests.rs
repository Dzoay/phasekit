//! The ABI's own tests (PLAN.md M10.4; ROT-148 to ROT-150, ROT-159), calling the exports as a C caller does.

#![allow(unsafe_code, reason = "the tests call the unsafe exports, each with valid buffers")]

use super::*;

/// A string's pointer and length.
fn s(text: &str) -> (*const c_char, usize) {
    (text.as_ptr().cast(), text.len())
}

/// This thread's last error.
fn last() -> String {
    let mut buf = [0 as c_char; 512];
    // SAFETY: `buf` is 512 writable bytes.
    let n = unsafe { pk_last_error(buf.as_mut_ptr(), buf.len()) };
    String::from_utf8_lossy(&buf[..n - 1].iter().map(|&c| c as u8).collect::<Vec<_>>()).into_owned()
}

/// A new registry's handle.
fn new_registry_handle() -> pk_registry {
    let mut handle = 0;
    // SAFETY: `handle` is writable.
    assert_eq!(unsafe { pk_registry_new(PK_DATASET_CORRECTED, &mut handle) }, PK_OK);
    handle
}

/// `PropsSI` through the ABI.
fn props(registry: pk_registry, output: &str, (n1, v1): (&str, f64), (n2, v2): (&str, f64), name: &str) -> (i32, f64) {
    let (mut value, (o, ol), (a, al), (b, bl), (f, fl)) = (f64::NAN, s(output), s(n1), s(n2), s(name));
    // SAFETY: every string is its own length's bytes; `value` is writable.
    let status = unsafe { pk_props_si(registry, o, ol, a, al, v1, b, bl, v2, f, fl, &mut value) };
    (status, value)
}

/// Rot: ROT-150 (map 11 F1, F3). A freed handle, a handle never issued and 0 are `PK_STALE_HANDLE` for every call, and
/// freeing twice is too; nothing else is touched.
#[test]
fn stale_handle_is_an_error() {
    let registry = new_registry_handle();
    let (status, t) = props(registry, "T", ("P", 101_325.0), ("Q", 0.0), "Water");
    assert!(status == PK_OK && (t - 373.124).abs() < 1e-3, "{status} {t}");
    assert_eq!(pk_registry_free(registry), PK_OK);
    for handle in [registry, 0, registry ^ (1 << 40), u64::MAX] {
        assert_eq!(props(handle, "T", ("P", 101_325.0), ("Q", 0.0), "Water").0, PK_STALE_HANDLE, "{handle:#x}");
        assert!(last().contains("stale"), "{}", last());
        assert_eq!(pk_registry_free(handle), PK_STALE_HANDLE);
        assert_eq!(pk_fluid_free(handle), PK_STALE_HANDLE);
    }
}

/// Rot: ROT-150 (map 11 F1). A call holds its own clone of what a handle names, so freeing the handle while another
/// thread uses it lets that call finish; every call after the free is `PK_STALE_HANDLE`. The fluid a registry made
/// outlives the registry.
#[cfg(not(target_family = "wasm"))]
#[test]
fn free_during_use_gives_stale_handle_status() {
    let registry = new_registry_handle();
    let mut fluid = 0;
    let (name, len) = s("Water");
    // SAFETY: the name is its length's bytes; `fluid` is writable.
    assert_eq!(unsafe { pk_fluid_new(registry, name, len, &mut fluid) }, PK_OK);
    let worker = std::thread::spawn(move || {
        let statuses: Vec<i32> = (0..200).map(|_| props(registry, "D", ("T", 300.0), ("P", 1e5), "Water").0).collect();
        statuses
    });
    assert_eq!(pk_registry_free(registry), PK_OK);
    let statuses = worker.join().unwrap_or_default();
    let first_stale = statuses.iter().position(|&s| s == PK_STALE_HANDLE).unwrap_or(statuses.len());
    assert!(statuses[..first_stale].iter().all(|&s| s == PK_OK), "{statuses:?}");
    assert!(statuses[first_stale..].iter().all(|&s| s == PK_STALE_HANDLE), "{statuses:?}");
    assert_eq!(props(registry, "D", ("T", 300.0), ("P", 1e5), "Water").0, PK_STALE_HANDLE);
    let mut buf = [0 as c_char; 16];
    let mut needed = 0;
    // SAFETY: `buf` is 16 writable bytes; `needed` is writable.
    assert_eq!(unsafe { pk_fluid_name(fluid, buf.as_mut_ptr(), buf.len(), &mut needed) }, PK_OK);
    assert_eq!((needed, buf[..5].iter().map(|&c| c as u8).collect::<Vec<_>>()), (6, b"Water".to_vec()));
    assert_eq!(pk_fluid_free(fluid), PK_OK);
}

/// Rot: ROT-150 (map 11 F3: CoolProp's `long` counter wraps into aliases). A slot's generation moves on at every free,
/// and the slot whose generation would wrap is retired, so a handle freed there never names a later value.
#[test]
fn handle_generation_wraps_without_aliasing() {
    let mut table = handles::Table::<u32>::starting_at(u32::MAX - 1);
    let (a, b) = (table.insert(1).unwrap_or(0), table.insert(2).unwrap_or(0));
    assert_eq!(a >> 32, u64::from(u32::MAX - 1));
    assert_eq!(table.remove(a).as_deref(), Some(&1));
    let c = table.insert(3).unwrap_or(0);
    assert_eq!((c >> 32, c & 0xffff_ffff), (u64::from(u32::MAX), a & 0xffff_ffff), "the slot's last generation");
    assert_eq!(table.remove(c).as_deref(), Some(&3));
    let d = table.insert(4).unwrap_or(0);
    assert_ne!(d & 0xffff_ffff, a & 0xffff_ffff, "the slot is retired, not wrapped to its first generation");
    for stale in [a, c] {
        assert!(table.get(stale).is_none() && table.remove(stale).is_none(), "{stale:#x}");
    }
    assert_eq!((table.get(b).as_deref(), table.get(d).as_deref()), (Some(&2), Some(&4)));
}

/// Rot: ROT-150 (map 11 F2). A string output reports the length it needs, NUL included; a shorter buffer is
/// `PK_BUFFER_TOO_SMALL` with only a NUL at its start and nothing past its length, a zero-length null buffer a pure size
/// query; `pk_last_error` returns its needed length whatever the buffer.
#[test]
fn too_small_buffer_returns_the_needed_length() {
    let registry = new_registry_handle();
    let (t, p, w) = (s("T"), s("P"), s("Water"));
    let phase = |buf: *mut c_char, len: usize, needed: &mut usize| {
        // SAFETY: the keys are their lengths' bytes; `buf` is null or `len` writable bytes.
        unsafe { pk_phase_si(registry, t.0, t.1, 300.0, p.0, p.1, 101_325.0, w.0, w.1, buf, len, needed) }
    };
    let (mut needed, mut small) = (0, [b'x' as c_char; 8]);
    assert_eq!(phase(std::ptr::null_mut(), 0, &mut needed), PK_BUFFER_TOO_SMALL);
    assert_eq!(needed, "liquid".len() + 1);
    assert_eq!(phase(small.as_mut_ptr(), 6, &mut needed), PK_BUFFER_TOO_SMALL);
    assert_eq!((small[0], small[1], small[6]), (0, b'x' as c_char, b'x' as c_char), "nothing past a NUL at the start");
    assert_eq!(phase(small.as_mut_ptr(), 7, &mut needed), PK_OK);
    assert_eq!(small[..7].iter().map(|&c| c as u8).collect::<Vec<_>>(), b"liquid\0");
    assert_eq!(props(registry, "X", ("T", 300.0), ("P", 1e5), "Water").0, PK_UNKNOWN_KEY);
    let message = "unknown output key `X`";
    // SAFETY: a null buffer of length 0 is a size query.
    assert_eq!(unsafe { pk_last_error(std::ptr::null_mut(), 0) }, message.len() + 1);
    let mut tiny = [b'x' as c_char; 4];
    // SAFETY: `tiny` is 4 writable bytes.
    assert_eq!(unsafe { pk_last_error(tiny.as_mut_ptr(), tiny.len()) }, message.len() + 1);
    assert_eq!((tiny[0], tiny[1]), (0, b'x' as c_char));
    assert_eq!(last(), message);
    assert_eq!(pk_registry_free(registry), PK_OK);
}

/// Rot: ROT-150 (map 11 F2, F16). A null string, output or out-parameter is `PK_NULL_POINTER`, a length over
/// PTRDIFF_MAX `PK_BAD_LENGTH`, bytes that are not UTF-8 `PK_BAD_UTF8`, an unknown data set `PK_INVALID_INPUT`, and an
/// unknown reference state `PK_UNKNOWN_KEY`; none writes its output.
#[test]
fn null_and_bad_lengths_return_status() {
    let registry = new_registry_handle();
    let (t, p, w) = (s("T"), s("P"), s("Water"));
    let mut value = 7.0;
    let call = |output: (*const c_char, usize), fluid: (*const c_char, usize), out: *mut f64| {
        // SAFETY: each pointer is null or its length's bytes; `out` is null or writable.
        unsafe { pk_props_si(registry, output.0, output.1, t.0, t.1, 300.0, p.0, p.1, 1e5, fluid.0, fluid.1, out) }
    };
    assert_eq!(call((std::ptr::null(), 1), w, &mut value), PK_NULL_POINTER);
    assert_eq!(call(s("D"), w, std::ptr::null_mut()), PK_NULL_POINTER);
    assert_eq!(call(s("D"), (w.0, usize::MAX), &mut value), PK_BAD_LENGTH);
    let bad = [0xff_u8, 0xfe];
    assert_eq!(call(s("D"), (bad.as_ptr().cast(), 2), &mut value), PK_BAD_UTF8);
    assert_eq!(value, 7.0, "no output written");
    let mut handle = 0;
    // SAFETY: `handle` is writable; a null out-parameter is refused.
    assert_eq!(unsafe { pk_registry_new(7, &mut handle) }, PK_INVALID_INPUT);
    // SAFETY: as above.
    assert_eq!(unsafe { pk_registry_new(PK_DATASET_PARITY, std::ptr::null_mut()) }, PK_NULL_POINTER);
    let (fluid, reference) = (s("Water"), s("ISO"));
    // SAFETY: the strings are their lengths' bytes; `handle` is writable.
    let status =
        unsafe { pk_registry_with_reference(registry, fluid.0, fluid.1, reference.0, reference.1, &mut handle) };
    assert_eq!((status, handle), (PK_UNKNOWN_KEY, 0));
    assert_eq!(pk_registry_free(registry), PK_OK);
}

/// Rot: ROT-150 (map 11 F16: exceptions escaping `extern "C"`). A panic inside an export's body never unwinds into the
/// caller: `guard` returns `PK_PANIC` and sets the thread's last error.
#[test]
fn panics_never_cross_the_boundary() {
    #[allow(clippy::panic, reason = "the panic the guard must catch")]
    let status = guard(|| panic!("a bug"));
    assert_eq!(status, PK_PANIC);
    assert!(last().contains("panic"), "{}", last());
    assert_eq!(guard(|| Ok(())), PK_OK);
    assert_eq!(last(), "", "success clears the last error");
}

/// Rot: ROT-148 (map 11 F5; CoolProp's process-wide errstring reports or swallows another thread's message). The last
/// error is per thread: a failure on one thread leaves another's empty, and each reads its own.
#[cfg(not(target_family = "wasm"))]
#[test]
fn pk_error_is_thread_local() {
    let registry = new_registry_handle();
    assert_eq!(props(registry, "T", ("P", 101_325.0), ("Q", 0.0), "Water").0, PK_OK);
    let other = std::thread::spawn(move || {
        let status = props(registry, "Y", ("T", 300.0), ("P", 1e5), "Water").0;
        (status, last())
    });
    let (status, message) = other.join().unwrap_or_default();
    assert_eq!((status, message.as_str()), (PK_UNKNOWN_KEY, "unknown output key `Y`"));
    assert_eq!(last(), "", "this thread's last call succeeded");
    assert_eq!(pk_registry_free(registry), PK_OK);
}

/// Rot: ROT-159 (map 11 §8). The default library exports `pk_*` symbols only, so it can share a process with a real
/// CoolProp; the CoolPropLib.h names are the separate shim build's (M10.6). Read from the built library with `nm`.
#[cfg(target_os = "linux")]
#[test]
#[allow(clippy::disallowed_methods, reason = "the test looks for the library cargo built beside it")]
fn default_library_exports_only_pk_symbols() {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let library =
        [dir.join("libphasekit_capi.so"), dir.join("../libphasekit_capi.so")].into_iter().find(|p| p.exists());
    let library = library.unwrap_or_else(|| dir.join("libphasekit_capi.so"));
    let nm = std::process::Command::new("nm").args(["-D", "--defined-only"]).arg(&library).output();
    let listing = nm.map(|out| String::from_utf8_lossy(&out.stdout).into_owned()).unwrap_or_default();
    let exported: Vec<&str> = listing.lines().filter_map(|l| l.split_whitespace().nth(2)).collect();
    let pk = ["pk_last_error", "pk_registry_new", "pk_registry_free", "pk_props_si", "pk_phase_si", "pk_fluid_new"];
    assert!(pk.iter().all(|name| exported.contains(name)), "{}: {exported:?}", library.display());
    let others: Vec<&&str> = exported.iter().filter(|name| !name.starts_with("pk_")).collect();
    assert_eq!(others, Vec::<&&str>::new(), "{}", library.display());
}

/// ROT-023: every batch `Status` and every kernel error has its own code, and a string-level failure is
/// `PK_UNKNOWN_KEY`; through the ABI, an invalid temperature is `PK_INVALID_INPUT`, an unknown fluid `PK_LOAD`, a
/// state outside the domain `PK_DOMAIN` and a phase's undefined property `PK_UNDEFINED`.
#[test]
fn every_status_has_its_code() {
    let codes = [
        (Status::Ok, PK_OK),
        (Status::Extrapolated, PK_EXTRAPOLATED),
        (Status::InvalidInput, PK_INVALID_INPUT),
        (Status::Unsupported, PK_UNSUPPORTED),
        (Status::Domain, PK_DOMAIN),
        (Status::NoConvergence, PK_NO_CONVERGENCE),
        (Status::Ambiguous, PK_AMBIGUOUS),
        (Status::Undefined, PK_UNDEFINED),
        (Status::NoModel, PK_NO_MODEL),
        (Status::Load, PK_LOAD),
        (Status::Other, PK_OTHER),
    ];
    for (status, code) in codes {
        assert_eq!(status_code(status), code, "{status:?}");
    }
    let unknown = Fail::from(CompatError::InvalidFluid("A&B".into()));
    assert_eq!((unknown.status, unknown.message.as_str()), (PK_UNKNOWN_KEY, "`A&B` is not a fluid name"));
    let registry = new_registry_handle();
    let cases = [
        (("T", -5.0), ("P", 1e5), "Water", PK_INVALID_INPUT),
        (("T", 300.0), ("P", 1e5), "Unobtainium", PK_LOAD),
        (("T", 200.0), ("P", 1e5), "Water", PK_DOMAIN),
        (("Q", 0.5), ("T", 300.0), "Water", PK_UNDEFINED),
    ];
    for (a, b, fluid, code) in cases {
        let output = if code == PK_UNDEFINED { "C" } else { "D" };
        assert_eq!(props(registry, output, a, b, fluid).0, code, "{a:?} {b:?} {fluid}");
    }
    assert_eq!(pk_registry_free(registry), PK_OK);
}

/// E10: `pk_registry_with_reference` makes a new registry in which the named fluid, under any of its names, reports in
/// the reference state; the registry it came from is unchanged, and `DEF` restores the model's own.
#[test]
fn registry_with_reference_reaches_the_strings() {
    let registry = new_registry_handle();
    let with = |base: pk_registry, reference: &str| {
        let ((f, fl), (r, rl), mut out) = (s("r134a"), s(reference), 0);
        // SAFETY: both strings are their lengths' bytes; `out` is writable.
        let status = unsafe { pk_registry_with_reference(base, f, fl, r, rl, &mut out) };
        assert_eq!(status, PK_OK, "{reference}: {}", last());
        out
    };
    // R134a's own data are in IIR already; ASHRAE puts h = 0 on the saturated liquid at −40 °C.
    let h = |r| props(r, "H", ("T", 233.15), ("Q", 0.0), "R134a");
    let ashrae = with(registry, "ASHRAE");
    assert!(h(ashrae).0 == PK_OK && h(ashrae).1.abs() < 1e-8, "{:?}", h(ashrae));
    let native = h(registry);
    assert!(native.0 == PK_OK && native.1 > 1e5, "{native:?}");
    let back = with(ashrae, "DEF");
    assert_eq!(h(back).1.to_bits(), native.1.to_bits());
    for handle in [back, ashrae, registry] {
        assert_eq!(pk_registry_free(handle), PK_OK);
    }
}
