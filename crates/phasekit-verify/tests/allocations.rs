//! Heap allocations on hot paths, counted by allocation-counter's global allocator (user decision AC1; ARCHITECTURE.md
//! §7: "no lock, global or allocation after first use"). One test binary, so nothing else allocates concurrently.
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use phasekit_core::Registry;

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
