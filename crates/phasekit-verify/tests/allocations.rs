//! Heap allocations on hot paths, counted by allocation-counter's global allocator (user decision AC1; ARCHITECTURE.md
//! §7: "no lock, global or allocation after first use"). One test binary, so nothing else allocates concurrently.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::{Order, Registry};
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

/// PLAN.md M3.8 (ARCHITECTURE.md §7: EOS ≤ 25 KiB per fluid; recorded at M3, enforced from M9): the compiled residual
/// part of every fluid without NonAnalytic terms keeps at most 25 KiB on the heap (measured: Methanol's 44 terms keep
/// 13,690 bytes, the most), and evaluating α^r to order 4 allocates nothing.
#[test]
fn compiled_residual_parts_fit_25_kib() {
    let registry = Registry::embedded().unwrap();
    let mut largest = (0, "");
    for f in phasekit_data::FLUIDS {
        let record = phasekit_core::internal::record(registry, f.name).unwrap();
        if !record.eos.non_analytic.is_empty() {
            continue;
        }
        let eos = term::residual_part(&record.eos);
        let mut kept = None;
        let built = allocation_counter::measure(|| kept = Some(term::residual_model(&record, &eos).unwrap()));
        let model = kept.unwrap();
        largest = largest.max((built.bytes_current, f.name));
        let (t, rho) = (record.eos.t_reducing, record.eos.rho_reducing);
        let call = allocation_counter::measure(|| {
            std::hint::black_box(model.eos().residual(std::hint::black_box(t), rho, Order::Four));
        });
        assert_eq!(call.count_total, 0, "{}: {call:?}", f.name);
    }
    assert!(largest.0 <= 25 * 1024, "{} keeps {} bytes", largest.1, largest.0);
}
