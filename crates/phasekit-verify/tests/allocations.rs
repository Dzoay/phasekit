//! Heap allocations on hot paths, counted by allocation-counter's global allocator (user decision AC1; ARCHITECTURE.md
//! §7: "no lock, global or allocation after first use"). One test binary, so nothing else allocates concurrently.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Input, Order, Pressure, Quality, Registry, Temperature};
use phasekit_verify::term;

/// PLAN.md M2.9: once a fluid is decoded, `Registry::get` by any of its names (any case) or its CAS number allocates
/// nothing: keys are compared case-folded byte by byte and the fluid is returned by reference (D8). A miss allocates
/// its error, and only that.
#[test]
fn hot_get_allocates_nothing() {
    let registry = Registry::embedded().unwrap();
    let keys = ["R134a", "r134A", "811-97-2", "LVGUZGTVOIAKKC-UHFFFAOYSA-N"]; // name, alias in another case, CAS, InChIKey
    for key in keys {
        registry.get(key).unwrap(); // the first get decodes
    }
    let hot = allocation_counter::measure(|| {
        for _ in 0..100 {
            for key in keys {
                std::hint::black_box(registry.get(std::hint::black_box(key)).is_ok());
            }
        }
    });
    assert_eq!(hot.count_total, 0, "{hot:?}");
    let miss = allocation_counter::measure(|| {
        std::hint::black_box(registry.get("unobtainium").is_err());
    });
    assert!(miss.count_total > 0, "the miss's error owns its name: {miss:?}");
}

/// PLAN.md M9.7 (ARCHITECTURE.md §6: per fluid, EOS ≤ 25 KiB and superancillary ≤ 40 KiB; recorded at M3 and M6,
/// checked from M9): the heap each fluid's compiled residual part and decoded superancillary keep, and that evaluating
/// them allocates nothing.
/// - EOS: every fluid within 25 KiB; Water's 56 terms keep 15 626 B on 64-bit targets, the most (M3.8 recorded
///   Methanol's 13 690 B as the most; the test asserted only the budget).
/// - Superancillary: within 40 KiB (user decision MB1); over the 25 KiB goal for 97 of the 130 (median 27 544 B,
///   PropyleneGlycol's 39 664 B), logged in issue #146. The three forward curves and their boundaries, what CoolProp
///   stores, fit 25 KiB for all but two fluids, PropyleneGlycol and one more; the stored T(ln p) inverse, kept, adds
///   about a quarter, and p(T)'s segment ends, kept for PQ (M12.9), 72 B for the median. With 32-bit pointers (wasm32)
///   each curve keeps 120 B less.
#[test]
fn eos_and_superancillary_bytes_per_fluid() {
    let registry = Registry::embedded().unwrap();
    let (mut largest_eos, mut sizes, mut forward_over) = ((0, ""), Vec::new(), Vec::new());
    for f in phasekit_data::FLUIDS {
        let record = phasekit_core::internal::record(registry, f.name).unwrap();
        let eos = term::residual_part(&record.eos);
        let mut kept = None;
        let built = allocation_counter::measure(|| kept = Some(term::residual_model(&record, &eos).unwrap()));
        let model = kept.unwrap();
        largest_eos = largest_eos.max((built.bytes_current, f.name));
        let (t, rho) = (record.eos.t_reducing, record.eos.rho_reducing);
        let call = allocation_counter::measure(|| {
            std::hint::black_box(model.eos().residual(std::hint::black_box(t), rho, Order::Four));
        });
        assert_eq!(call.count_total, 0, "{}: {call:?}", f.name);
        let mut curve = None;
        let built = allocation_counter::measure(|| curve = record.superancillary_curve());
        if let (Some(_), Some(sa)) = (curve, record.superancillary.as_ref()) {
            sizes.push((built.bytes_current, f.name));
            let forward = sa.curves.iter().map(|c| c.len() * 13 * 8).sum::<usize>() + sa.breaks.len() * 8;
            if forward > 25 * 1024 {
                forward_over.push(f.name);
            }
        }
    }
    assert!(largest_eos.0 <= 25 * 1024, "{largest_eos:?}");
    if cfg!(target_pointer_width = "64") {
        assert_eq!(largest_eos, (15_626, "Water"));
    }
    sizes.sort_unstable();
    let (largest, median) = (sizes[sizes.len() - 1], sizes[sizes.len() / 2].0);
    let over = sizes.iter().filter(|(bytes, _)| *bytes > 25 * 1024).count();
    let pointers = if cfg!(target_pointer_width = "64") { 0 } else { 120 };
    let want = (130, 27_544 - pointers, (39_664 - pointers, "PropyleneGlycol"), 97);
    assert_eq!((sizes.len(), median, largest, over), want);
    assert!(largest.0 <= 40 * 1024);
    assert!(forward_over.len() == 2 && forward_over.contains(&"PropyleneGlycol"), "{forward_over:?}");
    let water = registry.get("Water").unwrap();
    let quality = Quality::new(0.5).unwrap();
    let (qt, pq) =
        (Input::qt(quality, Temperature::new(400.0).unwrap()), Input::pq(Pressure::new(1e5).unwrap(), quality));
    for input in [qt, pq] {
        water.state(input).unwrap(); // decodes the curve
        let call = allocation_counter::measure(|| {
            std::hint::black_box(water.state(std::hint::black_box(input)).unwrap());
        });
        assert_eq!(call.count_total, 0, "{input:?}: {call:?}");
    }
}

/// `key` of /proc/self/status (`VmRSS:`, `RssAnon:`, `RssFile:`), in KiB.
#[cfg(target_os = "linux")]
#[allow(clippy::disallowed_methods, reason = "a Linux test reads its own process's memory counters")]
fn status_kib(key: &str) -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status.lines().find_map(|l| l.strip_prefix(key)).unwrap();
    line.trim().trim_end_matches("kB").trim().parse().unwrap()
}

/// PLAN.md M9.7 (ARCHITECTURE.md §6: all 136 loaded ≤ 16 MiB RSS, ≤ 8 MiB of it heap): one `Registry::get` of every
/// embedded fluid, in this test's own process (nextest), measured from /proc/self/status around the loads. The heap grows
/// by 6.1 MiB (`RssAnon`), within 8 MiB; RSS by 15.3 MiB, as loading reads every byte of each fluid's blob for its
/// checksum and so maps 9.2 MiB of read-only blob pages (`RssFile`; the blobs total 9.0 MB). RSS is within 16 MiB (user
/// decision MB1); the 8 MiB goal is logged in issue #146. CoolProp: 104.5 MiB for one state of every fluid (the C++
/// baseline).
#[cfg(target_os = "linux")]
#[test]
fn all_136_loaded_rss() {
    let (rss, anon) = (status_kib("VmRSS:"), status_kib("RssAnon:"));
    let registry = Registry::embedded().unwrap();
    for f in phasekit_data::FLUIDS {
        std::hint::black_box(registry.get(f.name).unwrap());
    }
    let (rss, anon) = (status_kib("VmRSS:") - rss, status_kib("RssAnon:") - anon);
    assert_eq!(phasekit_data::FLUIDS.len(), 136);
    assert!(anon <= 8 * 1024, "heap {anon} KiB over 8 MiB");
    assert!(rss <= 16 * 1024, "RSS {rss} KiB over 16 MiB (issue #146)");
}

/// PLAN.md M9.7 (ARCHITECTURE.md §3.8 and §6: per state ≤ 256 B on the stack): `State` is 208 B on 64-bit targets,
/// recorded here, and within 256 B on every target (as `lib.rs` asserts at compile time).
#[test]
fn state_fits_256_bytes() {
    let size = std::mem::size_of::<phasekit_core::State>();
    assert!(size <= 256, "{size} B");
    if cfg!(target_pointer_width = "64") {
        assert_eq!(size, 208);
    }
}

/// ROT-027 (caloric part), PLAN.md M5.2a: the caloric curves are datagen output, so the first query of a decoded record
/// evaluates them and builds nothing: zero allocations, where CoolProp builds them at first use (45-63 ms per fluid
/// behind a mutex, map 03 §6).
#[test]
fn first_caloric_query_builds_nothing() {
    let record = phasekit_core::internal::record(Registry::embedded().unwrap(), "Water").unwrap();
    let curves = record.caloric_view().unwrap(); // the freshness gate, decided once
    let mut first = None;
    let counted = allocation_counter::measure(|| first = curves.at(std::hint::black_box(400.0)));
    assert_eq!(counted.count_total, 0, "{counted:?}");
    assert!(first.is_some_and(|values| values.iter().all(|v| v.is_finite())));
}

/// ARCHITECTURE.md §7, ROT-014 (PLAN.md M5.8): once Water is decoded and its saturation curve materialised, a DT flash
/// allocates nothing, single-phase or two-phase, nor does reading its properties.
#[test]
fn dt_flash_allocates_nothing() {
    let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let water = registry.get("Water").unwrap();
    let inputs = [(10.0, 700.0), (55_400.0, 300.0), (1_000.0, 400.0)].map(|(rho, t)| {
        phasekit_core::Input::dt(
            phasekit_core::Density::molar(rho).unwrap(),
            phasekit_core::Temperature::new(t).unwrap(),
        )
    });
    water.state(inputs[2]).unwrap(); // decodes the superancillary
    let counted = allocation_counter::measure(|| {
        for input in inputs {
            let state = water.state(std::hint::black_box(input)).unwrap();
            std::hint::black_box((
                state.p(),
                state.h(phasekit_core::Basis::Molar),
                state.cp(phasekit_core::Basis::Molar).ok(),
            ));
        }
    });
    assert_eq!(counted.count_total, 0, "{counted:?}");
}

/// ARCHITECTURE.md §7 (PLAN.md M5.8): after the first point has materialised what it needs, a batch of DT points into
/// caller-owned buffers allocates nothing per point, refusals (`Undefined` Q, cp in the dome) included.
#[test]
fn batch_point_allocates_nothing() {
    use phasekit_core::batch::{self, BatchRequest, Status};
    use phasekit_core::{Basis, Pair, Prop};
    let registry = Registry::from_embedded(phasekit_core::DataSet::Corrected).unwrap();
    let water = registry.get("Water").unwrap();
    let (x, y) = ([10.0, 55_400.0, 1_000.0, 20.0], [700.0, 300.0, 400.0, 500.0]);
    let outputs = [Prop::P, Prop::Hmolar, Prop::Cpmolar, Prop::Q];
    let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &outputs);
    let (mut out, mut status) = ([0.0; 16], [Status::Other; 16]);
    batch::evaluate(water, &req, &mut out, &mut status).unwrap(); // warm-up: decodes the superancillary
    let counted = allocation_counter::measure(|| {
        std::hint::black_box(batch::evaluate(water, &req, &mut out, &mut status).unwrap());
    });
    assert_eq!(counted.count_total, 0, "{counted:?}");
    assert!(status.contains(&Status::Undefined) && status.contains(&Status::Ok), "{status:?}");
}
