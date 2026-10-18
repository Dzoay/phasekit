//! The ABI's own tests (PLAN.md M10.4; ROT-148 to ROT-150, ROT-159), calling the exports as a C caller does.

#![allow(unsafe_code, reason = "the tests call the unsafe exports, each with valid buffers")]
// The shim's build exports no pk_* function. A plain `#[cfg(test)]` on the module keeps cargo-mutants out of it.
#![cfg(not(feature = "coolproplib-shim"))]

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
/// caller: `guard` returns `PK_PANIC` and sets the thread's last error. Where a panic aborts (wasm32-wasip2), there is
/// no unwind to catch.
#[cfg(panic = "unwind")]
#[test]
fn panics_never_cross_the_boundary() {
    #[allow(clippy::panic, reason = "the panic the guard must catch")]
    let status = guard(|| panic!("a bug"));
    assert_eq!(status, PK_PANIC);
    assert!(last().contains("panic"), "{}", last());
    assert_eq!(guard(|| Ok(())), PK_OK);
    assert_eq!(last(), "", "success clears the last error");
}

/// `status` is `PK_PANIC` with the thread's last error naming the caught panic.
fn caught(status: i32) -> bool {
    status == PK_PANIC && last().contains("a panic was caught")
}

/// PLAN.md M24.1 (issue #219): a panic where a call reaches the registry or a model, for one of
/// `phasekit_compat::fault`'s names, unwinds through compat and the registry to the export's catch. Every export a
/// fluid name or a fluid handle reaches returns `PK_PANIC` with the last error set: the registry's resolver panics for
/// `fault::resolve`, the model's `info` for `fault::info`, its flash for `fault::flash` and its drop for `fault::drop`.
/// The thread's next call succeeds, and a fluid freed by a panicking drop is gone.
#[cfg(panic = "unwind")]
#[test]
fn a_panic_on_the_call_path_is_pk_panic() {
    use phasekit_compat::fault::{DROP, FLASH, INFO, RESOLVE};
    let registry = new_registry_handle();
    for name in [RESOLVE, FLASH] {
        assert!(caught(props(registry, "Hmass", ("T", 300.0), ("P", 1e5), name).0), "pk_props_si {name}: {}", last());
    }
    let ((r, rl), (i, il), (f, fl)) = (s(RESOLVE), s(INFO), s(FLASH));
    let (mut buf, mut needed, mut value, mut handle) = ([0 as c_char; 64], 0, 0.0, 0);
    // SAFETY (each): every string is its own length's bytes; every output is writable for what the call writes.
    unsafe {
        assert!(caught(pk_props1_si(registry, s("Tcrit").0, 5, r, rl, &mut value)), "pk_props1_si");
        let phase =
            pk_phase_si(registry, s("T").0, 1, 300.0, s("P").0, 1, 1e5, f, fl, buf.as_mut_ptr(), 64, &mut needed);
        assert!(caught(phase), "pk_phase_si");
        assert!(caught(pk_fluid_new(registry, r, rl, &mut handle)), "pk_fluid_new");
        assert!(caught(pk_registry_with_reference(registry, i, il, s("IIR").0, 3, &mut handle)), "with_reference");
    }
    let (info, flash, drop) =
        (fluid_handle(registry, INFO), fluid_handle(registry, FLASH), fluid_handle(registry, DROP));
    let ((keys, lens), (t, p), (mut out, mut cells)) = (([s("Hmass").0], [5]), ([300.0], [1e5]), ([0.0], [0]));
    // SAFETY (each): every string and array is its own length's; every output is writable for what the call writes.
    unsafe {
        assert!(caught(pk_fluid_name(info, buf.as_mut_ptr(), 64, &mut needed)), "pk_fluid_name");
        let (o, st) = (out.as_mut_ptr(), cells.as_mut_ptr());
        let (tk, pk) = (s("T").0, s("P").0);
        let batch = pk_batch(flash, keys.as_ptr(), lens.as_ptr(), 1, tk, 1, t.as_ptr(), pk, 1, p.as_ptr(), 1, o, st, 1);
        assert!(caught(batch), "pk_batch");
    }
    assert!(caught(pk_fluid_free(drop)), "pk_fluid_free");
    assert_eq!(pk_fluid_free(drop), PK_STALE_HANDLE, "the handle was freed before the drop panicked");
    assert_eq!(props(registry, "T", ("P", 101_325.0), ("Q", 0.0), "Water").0, PK_OK);
    assert_eq!(last(), "", "the next call succeeds");
    for handle in [info, flash] {
        assert_eq!(pk_fluid_free(handle), PK_OK);
    }
    assert_eq!(pk_registry_free(registry), PK_OK);
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

/// A new fluid handle for `name` in `registry`.
fn fluid_handle(registry: pk_registry, name: &str) -> pk_fluid {
    let (mut handle, (n, nl)) = (0, s(name));
    // SAFETY: `name` is its own length's bytes; `handle` is writable.
    assert_eq!(unsafe { pk_fluid_new(registry, n, nl, &mut handle) }, PK_OK);
    handle
}

/// A batch's `out` and `status` buffers and their cell count.
type Cells = (*mut f64, *mut i32, usize);

/// `pk_batch` with `T` = `t` and the input key `name2` = `p`, `n` points and `cells` cells into `out` and `status`.
///
/// # Safety
/// `t` and `p` hold `n` values or `n` is over PTRDIFF_MAX doubles; `out` and `status` are null or hold `cells`.
unsafe fn batch(
    fluid: pk_fluid,
    keys: &[&str],
    (t, (name2, p)): (&[f64], (&str, &[f64])),
    n: usize,
    cells: Cells,
) -> i32 {
    let (ptrs, lens): (Vec<_>, Vec<_>) = keys.iter().map(|k| s(k)).unzip();
    let ((a, al), (b, bl), (out, status, cells)) = (s("T"), s(name2), cells);
    // SAFETY: the keys and names are their own lengths' bytes; the rest is this function's contract.
    unsafe {
        pk_batch(
            fluid,
            ptrs.as_ptr(),
            lens.as_ptr(),
            keys.len(),
            a,
            al,
            t.as_ptr(),
            b,
            bl,
            p.as_ptr(),
            n,
            out,
            status,
            cells,
        )
    }
}

/// PLAN.md M10.5 (map 11 F15, F23; user decision 11). `pk_batch` on a `pk_fluid` is `batch::evaluate` on its fluid, bit
/// for bit and status for status: point-major `out[i*M + k]`, each cell's status its `PK_*` code. `T` before `P`, in
/// PropsSI's order, is the kernel's `PT` with the operands swapped. T ≤ 0, NaN and a pressure over the model's limit
/// fail their points' cells, NaN with their codes; the other points succeed.
#[test]
fn pk_batch_equals_rust_batch_bitwise() {
    use phasekit_core::batch::{BatchRequest, evaluate};
    use phasekit_core::{Basis, DerivVar, Pair, Partial, Prop};
    let registry = new_registry_handle();
    let fluid = fluid_handle(registry, "Water");
    let keys = ["T", "Hmass", "Dmolar", "Q", "viscosity", "d(P)/d(T)|Dmolar", "Cvmass"];
    let dp_dt = Prop::Partial(Partial { of: DerivVar::P, wrt: DerivVar::T, at: DerivVar::Dmolar });
    let props = [Prop::T, Prop::Hmass, Prop::Dmolar, Prop::Q, Prop::Viscosity, dp_dt, Prop::Cvmass];
    let t = [300.0, 500.0, 700.0, 1000.0, -1.0, f64::NAN, 400.0];
    let p = [1e5, 1e6, 2e7, 1e8, 1e5, 1e5, 1e10];
    let (n, m) = (t.len(), keys.len());
    let (mut out, mut status) = (vec![0.0; n * m], vec![-1; n * m]);
    // SAFETY: `t` and `p` hold `n` values; `out` and `status` hold n x m cells.
    let call = unsafe { batch(fluid, &keys, (&t, ("P", &p)), n, (out.as_mut_ptr(), status.as_mut_ptr(), n * m)) };
    assert_eq!((call, last()), (PK_OK, String::new()));
    let registry_rs = Registry::from_embedded(DataSet::Corrected).unwrap();
    let (mut want, mut cells) = (vec![0.0; n * m], vec![Status::Ok; n * m]);
    let req = BatchRequest::new(Pair::PT, Basis::Molar, &p, &t, &props);
    evaluate(registry_rs.get("Water").unwrap(), &req, &mut want, &mut cells).unwrap();
    assert_eq!(
        out.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        want.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
    assert_eq!(status, cells.into_iter().map(status_code).collect::<Vec<_>>());
    for code in [PK_OK, PK_INVALID_INPUT, PK_DOMAIN] {
        assert!(status.contains(&code), "no cell is {code}: {status:?}");
    }
    assert_eq!((pk_fluid_free(fluid), pk_registry_free(registry)), (PK_OK, PK_OK));
}

/// PLAN.md M10.5 (map 11 F2). A bad call to `pk_batch` is refused whole, before any cell is written: an unknown output
/// key, `Phase` (not a property), two keys that form no pair, `cells` other than n x m, more points than any buffer
/// holds, a null or misaligned buffer, a stale fluid. With no points, null buffers and no cells make a good call.
#[test]
fn pk_batch_refuses_a_bad_call_whole() {
    let registry = new_registry_handle();
    let fluid = fluid_handle(registry, "Water");
    let (t, p) = ([300.0, 400.0], [1e5, 1e5]);
    let (mut out, mut status) = ([7.0; 4], [-9; 4]);
    let (o, st) = (out.as_mut_ptr(), status.as_mut_ptr());
    let misaligned = o.cast::<u8>().wrapping_add(1).cast::<f64>();
    let cases: [(&[&str], &str, usize, Cells, i32); 8] = [
        (&["Hmass", "X"], "P", 2, (o, st, 4), PK_UNKNOWN_KEY),
        (&["Phase"], "P", 2, (o, st, 2), PK_UNKNOWN_KEY),
        (&["Hmass"], "T", 2, (o, st, 2), PK_UNKNOWN_KEY),
        (&["Hmass", "P"], "P", 2, (o, st, 3), PK_BAD_LENGTH),
        (&["Hmass"], "P", usize::MAX, (o, st, 4), PK_BAD_LENGTH),
        (&["Hmass", "P"], "P", 2, (std::ptr::null_mut(), st, 4), PK_NULL_POINTER),
        (&["Hmass", "P"], "P", 2, (o, std::ptr::null_mut(), 4), PK_NULL_POINTER),
        (&["Hmass", "P"], "P", 2, (misaligned, st, 4), PK_NULL_POINTER),
    ];
    for (keys, name2, n, cells, want) in cases {
        // SAFETY: `t` and `p` hold 2 values, or `n` is over PTRDIFF_MAX doubles; `out` and `status` hold 4 cells.
        let got = unsafe { batch(fluid, keys, (&t, (name2, &p)), n, cells) };
        assert_eq!(got, want, "{keys:?} {name2} {n}: {}", last());
    }
    assert_eq!(pk_fluid_free(fluid), PK_OK);
    // SAFETY: as above.
    let stale = unsafe { batch(fluid, &["Hmass", "P"], (&t, ("P", &p)), 2, (o, st, 4)) };
    assert_eq!((stale, out, status), (PK_STALE_HANDLE, [7.0; 4], [-9; 4]), "no cell is written");
    let (none, fluid) = (std::ptr::null_mut(), fluid_handle(registry, "Water"));
    // SAFETY: no points and no cells: nothing is read or written.
    let empty = unsafe { batch(fluid, &["Hmass"], (&[], ("P", &[])), 0, (none, none.cast(), 0)) };
    assert_eq!((empty, last()), (PK_OK, String::new()));
    assert_eq!(pk_fluid_free(fluid), PK_OK);
    assert_eq!(pk_registry_free(registry), PK_OK);
}

/// Every export, by name.
#[cfg(any(
    all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "windows", target_arch = "x86_64")
))]
const EXPORTS: [&str; 11] = [
    "pk_props_si",
    "pk_props1_si",
    "pk_phase_si",
    "pk_batch",
    "pk_fluid_name",
    "pk_registry_with_reference",
    "pk_registry_new",
    "pk_registry_free",
    "pk_fluid_new",
    "pk_fluid_free",
    "pk_last_error",
];

/// Calls the export `name` with good arguments on `registry` and its Water `fluid`: its status, or -1 if `pk_props_si`'s
/// value is not `reference`'s bits. `handle` carries a new registry, then a new fluid, from one call to the next.
#[cfg(any(
    all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "windows", target_arch = "x86_64")
))]
fn call_export(name: &str, (registry, fluid, reference): (pk_registry, pk_fluid, u64), handle: &mut u64) -> i32 {
    let ((w, wl), (t, p), mut buf, mut needed, mut value) = (s("Water"), ([300.0], [1e5]), [0 as c_char; 64], 0, 0.0);
    let ((keys, lens), (mut out, mut cells)) = (([s("Hmass").0], [5]), ([0.0], [0]));
    // SAFETY (each): every string is its own length's bytes; every output is writable for what the call writes.
    unsafe {
        match name {
            "pk_props_si" => {
                let (status, h) = props(registry, "Hmass", ("T", 300.0), ("P", 1e5), "Water");
                if h.to_bits() == reference { status } else { -1 }
            }
            "pk_props1_si" => pk_props1_si(registry, s("Tcrit").0, 5, w, wl, &mut value),
            "pk_phase_si" => {
                pk_phase_si(registry, s("T").0, 1, 300.0, s("P").0, 1, 1e5, w, wl, buf.as_mut_ptr(), 64, &mut needed)
            }
            "pk_batch" => {
                let (o, st) = (out.as_mut_ptr(), cells.as_mut_ptr());
                pk_batch(
                    fluid,
                    keys.as_ptr(),
                    lens.as_ptr(),
                    1,
                    s("T").0,
                    1,
                    t.as_ptr(),
                    s("P").0,
                    1,
                    p.as_ptr(),
                    1,
                    o,
                    st,
                    1,
                )
            }
            "pk_fluid_name" => pk_fluid_name(fluid, buf.as_mut_ptr(), 64, &mut needed),
            "pk_registry_with_reference" => {
                pk_registry_with_reference(registry, s("R134a").0, 5, s("ASHRAE").0, 6, handle)
            }
            "pk_registry_new" => pk_registry_free(*handle).max(pk_registry_new(PK_DATASET_PARITY, handle)),
            "pk_registry_free" => pk_registry_free(*handle),
            "pk_fluid_new" => pk_fluid_new(registry, w, wl, handle),
            "pk_fluid_free" => pk_fluid_free(*handle),
            _ => i32::try_from(pk_last_error(buf.as_mut_ptr(), 64)).unwrap_or(-1) - 1,
        }
    }
}

/// Rot: ROT-151 (map 11 F17; PLAN.md M10.7). Every export runs round-to-nearest with every trap masked, and gives the
/// caller its environment back. A caller rounding toward zero, with FE_OVERFLOW raised and FE_INVALID's trap enabled
/// (where the hardware traps: aarch64 cores need not), gets `pk_props_si`'s round-to-nearest bits, and finds its
/// rounding mode, flags and traps as they were after every export.
#[cfg(any(
    all(target_os = "linux", target_env = "gnu", any(target_arch = "x86_64", target_arch = "aarch64")),
    all(target_os = "windows", target_arch = "x86_64")
))]
#[test]
fn pk_exports_restore_the_callers_fp_environment() {
    use crate::fp::sys::probe::{
        INVALID, OVERFLOW, TOWARD_ZERO, clear_flags, flags, raise_overflow, rounding, set_rounding, set_traps, traps,
    };
    let (registry, nearest, mut handle) = (new_registry_handle(), rounding(), 0);
    let fluid = fluid_handle(registry, "Water");
    let reference = props(registry, "Hmass", ("T", 300.0), ("P", 1e5), "Water").1.to_bits();
    set_rounding(TOWARD_ZERO);
    clear_flags();
    raise_overflow();
    set_traps(INVALID);
    let host = (rounding(), flags(), traps());
    let after: Vec<(&str, i32, (i32, i32, i32))> = EXPORTS
        .iter()
        .map(|name| {
            (*name, call_export(name, (registry, fluid, reference), &mut handle), (rounding(), flags(), traps()))
        })
        .collect();
    set_traps(0);
    clear_flags();
    set_rounding(nearest);
    assert!(host.0 == TOWARD_ZERO && host.1 & OVERFLOW != 0, "the host's rounding and flag: {host:?}");
    assert!(host.2 == INVALID || cfg!(target_arch = "aarch64"), "the trap was not enabled: {host:?}");
    for (name, status, environment) in after {
        assert_eq!((status, environment), (PK_OK, host), "{name}");
    }
    assert_eq!(pk_registry_free(registry), PK_OK);
}
