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

/// PLAN.md M3.8 (ARCHITECTURE.md §7: EOS ≤ 25 KiB per fluid; recorded at M3, enforced from M9): the compiled residual
/// part of every fluid keeps at most 25 KiB on the heap (measured: Methanol's 44 terms keep
/// 13,690 bytes, the most), and evaluating α^r to order 4 allocates nothing.
#[test]
fn compiled_residual_parts_fit_25_kib() {
    let registry = Registry::embedded().unwrap();
    let mut largest = (0, "");
    for f in phasekit_data::FLUIDS {
        let record = phasekit_core::internal::record(registry, f.name).unwrap();
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

/// PLAN.md M6.11 (ARCHITECTURE.md §7: superancillary ≤ 25 KiB per fluid; recorded at M6, enforced from M9): the heap a
/// decoded superancillary keeps, recorded: median 27 472 B over the 130, PropyleneGlycol's 39 576 B the most, 97 of
/// them above the 25 KiB target (the stored inverse and extrema on top of CoolProp's three curves), which M9.7 meets by
/// optimisation or a recorded re-plan; pinned below 40 KiB until then. With 32-bit pointers (wasm32) each curve keeps
/// 112 B less. QT and PQ on the decoded curve allocate nothing.
#[test]
fn superancillary_bytes_are_recorded() {
    let registry = Registry::embedded().unwrap();
    let mut sizes = Vec::new();
    for f in phasekit_data::FLUIDS {
        let record = phasekit_core::internal::record(registry, f.name).unwrap();
        let mut kept = None;
        let built = allocation_counter::measure(|| kept = record.superancillary_curve());
        if kept.is_some() {
            sizes.push((built.bytes_current, f.name));
        }
    }
    sizes.sort_unstable();
    let (largest, median) = (sizes[sizes.len() - 1], sizes[sizes.len() / 2].0);
    let over = sizes.iter().filter(|(bytes, _)| *bytes > 25 * 1024).count();
    let pointers = if cfg!(target_pointer_width = "64") { 0 } else { 112 };
    let want = (130, 27_472 - pointers, (39_576 - pointers, "PropyleneGlycol"), 97);
    assert_eq!((sizes.len(), median, largest, over), want);
    assert!(largest.0 <= 40 * 1024);
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
