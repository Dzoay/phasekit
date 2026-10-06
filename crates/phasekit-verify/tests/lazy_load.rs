//! "Load only what is needed" as tests, not claims (Kernel graft). Counting `DataSource`s prove that each
//! fluid is read at most once per registry (also under 16 racing threads), that failures are cached, that
//! cross-fluid references are resolved and checked when a layer is built but decoded only on first need
//! (E5), that a `Fluid` keeps working after its registry is gone (S-05), and that layers share fluids
//! instead of re-decoding them (E6).
#![allow(clippy::unwrap_used)] // test-crate helpers outside #[test] fns (unwrap is denied in library code)

use std::sync::Arc;
#[allow(clippy::disallowed_types, reason = "a test-only load counter; the library has no atomics (ROT-031)")]
use std::sync::atomic::{AtomicUsize, Ordering};

use phasekit_core::internal::FluidRecord;
use phasekit_core::{
    Blob, DataSet, DataSource, Density, Error, FlashOptions, FluidId, Input, LoadError, Order, Pack, Phase, Prop,
    Registry, Temperature,
};

/// One test fluid: names (canonical first), declared references, and whether its blob decodes.
type Spec = (&'static [&'static str], &'static [&'static str], bool);

#[derive(Debug)]
#[allow(clippy::disallowed_types, reason = "a test-only load counter; the library has no atomics (ROT-031)")]
struct Counting {
    fluids: Vec<Spec>,
    reads: Vec<AtomicUsize>,
}

/// A source whose counters the test can still read after a registry owns it.
#[derive(Debug, Clone)]
struct Shared(Arc<Counting>);

impl Shared {
    #[allow(clippy::disallowed_types, reason = "a test-only load counter; the library has no atomics (ROT-031)")]
    fn new(fluids: &[Spec]) -> Self {
        Shared(Arc::new(Counting {
            fluids: fluids.to_vec(),
            reads: fluids.iter().map(|_| AtomicUsize::new(0)).collect(),
        }))
    }
    fn reads(&self) -> Vec<usize> {
        self.0.reads.iter().map(|r| r.load(Ordering::SeqCst)).collect()
    }
    fn spec(&self, id: FluidId) -> Result<&Spec, LoadError> {
        self.0.fluids.get(id.0 as usize).ok_or(LoadError::Format("id".into()))
    }
}

impl DataSource for Shared {
    fn names(&self) -> Vec<Vec<String>> {
        self.0.fluids.iter().map(|(names, _, _)| names.iter().map(|n| n.to_string()).collect()).collect()
    }
    fn blob(&self, id: FluidId) -> Result<Blob, LoadError> {
        self.0.reads[id.0 as usize].fetch_add(1, Ordering::SeqCst);
        let (names, _, decodes) = self.spec(id)?;
        let bytes =
            if *decodes { FluidRecord::synthetic(names[0]).unwrap().encode() } else { b"PKITBLOB\x02".to_vec() };
        Ok(Blob::Shared(bytes.into()))
    }
    fn references(&self, id: FluidId) -> Vec<String> {
        self.spec(id).map(|(_, refs, _)| refs.iter().map(|r| r.to_string()).collect()).unwrap_or_default()
    }
}

fn layer(base: &Registry, source: &Shared) -> Result<Registry, Error> {
    base.with_source(Box::new(source.clone()), DataSet::Corrected)
}

fn gas_state(reg: &Registry, name: &str) -> phasekit_core::State {
    let input = Input::dt(Density::molar(100.0).unwrap(), Temperature::new(300.0).unwrap());
    reg.get(name).unwrap().flash(input, &FlashOptions::new().with_phase(Phase::Gas)).unwrap()
}

/// `f(k)` on 16 threads, k = 0..16, each held at a barrier until all 16 have started, so their first touches overlap
/// instead of running one after another as the threads spawn. The results, in thread order.
#[cfg(not(target_family = "wasm"))]
fn race<T: Send>(f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let start = std::sync::Barrier::new(16);
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..16)
            .map(|k| {
                let (start, f) = (&start, &f);
                s.spawn(move || {
                    start.wait();
                    f(k)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

#[test]
fn nothing_is_read_until_asked_and_failures_are_cached() {
    let source = Shared::new(&[(&["Alpha", "A-1"], &[], false), (&["Beta"], &[], false)]);
    let reg = layer(&Registry::empty(), &source).unwrap();
    assert_eq!(reg.canonical_name("a-1"), Some("Alpha"));
    assert_eq!((source.reads(), reg.loaded().count()), (vec![0, 0], 0));
    let first = reg.get("ALPHA").unwrap_err();
    let again = reg.clone().get("alpha").unwrap_err(); // clones share the layers
    assert_eq!(first, again);
    assert!(matches!(first, Error::Load(LoadError::Format(_)))); // a truncated blob
    assert_eq!(source.reads(), [1, 0]);
    assert!(matches!(reg.get("gamma"), Err(Error::Load(LoadError::UnknownName(_)))));
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn first_touch_initialises_once_under_contention() {
    let source = Shared::new(&[(&["Alpha"], &[], true), (&["Beta"], &[], true)]);
    let reg = layer(&Registry::empty(), &source).unwrap();
    assert!(race(|_| reg.get("Beta").is_ok()).into_iter().all(|ok| ok));
    assert_eq!(source.reads(), [0, 1]);
    assert_eq!(reg.loaded().collect::<Vec<_>>(), ["Beta"]);
}

#[test]
fn colliding_names_are_refused_within_and_across_layers() {
    let dup = Shared::new(&[(&["X"], &[], true), (&["x"], &[], true)]);
    assert_eq!(layer(&Registry::empty(), &dup).unwrap_err(), Error::Load(LoadError::DuplicateName("x".into())));
    let base = layer(&Registry::empty(), &Shared::new(&[(&["Alpha"], &[], true)])).unwrap();
    let clash = Shared::new(&[(&["ALPHA"], &[], true)]);
    assert_eq!(layer(&base, &clash).unwrap_err(), Error::Load(LoadError::DuplicateName("alpha".into())));
}

/// E5: references are checked when the layer is built (CoolProp fails at the first transport call, map 05
/// R7), and a cycle is refused instead of deadlocking on re-entrant lazy initialisation.
#[test]
fn references_are_resolved_when_the_layer_is_built() {
    let missing = Shared::new(&[(&["A"], &["Nope"], true)]);
    let want = LoadError::MissingReference { fluid: "A".into(), reference: "Nope".into() };
    assert_eq!(layer(&Registry::empty(), &missing).unwrap_err(), Error::Load(want));
    let cycle = Shared::new(&[(&["A"], &["b"], true), (&["B"], &["A"], true), (&["C"], &[], true)]);
    assert!(matches!(layer(&Registry::empty(), &cycle), Err(Error::Load(LoadError::ReferenceCycle(_)))));
    let selfish = Shared::new(&[(&["A"], &["A"], true)]);
    assert_eq!(layer(&Registry::empty(), &selfish).unwrap_err(), Error::Load(LoadError::ReferenceCycle("A".into())));
    assert_eq!(missing.reads(), [0]);
}

/// E5 + S-05: an A → B (ECS) edge reads B zero times on the thermo path and once on the first transport
/// call, and the `Fluid` keeps working after its registry is dropped (no `Detached`).
#[test]
fn a_reference_is_read_once_on_first_need_and_outlives_its_registry() {
    let source = Shared::new(&[(&["A"], &["B"], true), (&["B"], &[], true)]);
    let reg = layer(&Registry::empty(), &source).unwrap();
    let a = reg.get("A").unwrap().clone();
    let state = gas_state(&reg, "A");
    assert_eq!(source.reads(), [1, 0]); // thermo never touches B
    drop(reg);
    let viscosity = a.prop(&state, Prop::Viscosity);
    assert_eq!(viscosity, Err(Error::NoModel { prop: Prop::Viscosity })); // sketch: ECS evaluation lands at M8
    assert_eq!(source.reads(), [1, 1]);
    let _ = a.prop(&state, Prop::Viscosity);
    assert_eq!(source.reads(), [1, 1]);
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn a_reference_is_read_once_under_contention() {
    let source = Shared::new(&[(&["A"], &["B"], true), (&["B"], &[], true)]);
    let reg = layer(&Registry::empty(), &source).unwrap();
    let state = gas_state(&reg, "A");
    let viscosities = race(|_| reg.get("A").unwrap().prop(&state, Prop::Viscosity));
    assert!(viscosities.iter().all(|v| *v == Err(Error::NoModel { prop: Prop::Viscosity })), "{viscosities:?}");
    assert_eq!(source.reads(), [1, 1]);
}

/// E6: a browser adds a fetched pack as a new layer. A request before the fetch is a plain, uncached miss;
/// afterwards the new fluid resolves, its reference into the base layer is shared, and nothing in the base
/// layer is decoded again.
#[test]
fn layers_share_fluids_and_misses_are_not_cached() {
    let base_source = Shared::new(&[(&["Alpha"], &[], true)]);
    let base = layer(&Registry::empty(), &base_source).unwrap();
    gas_state(&base, "Alpha");
    assert!(matches!(base.get("Gamma"), Err(Error::Load(LoadError::UnknownName(_)))));
    let pack = Shared::new(&[(&["Gamma"], &["Alpha"], true)]);
    let page = layer(&base, &pack).unwrap();
    let state = gas_state(&page, "Gamma");
    let _ = page.get("Gamma").unwrap().prop(&state, Prop::Viscosity);
    gas_state(&page, "Alpha");
    assert_eq!((base_source.reads(), pack.reads()), (vec![1], vec![1]));
    assert!(base.get("Gamma").is_err()); // the base registry value is unchanged
}

/// E5: a data fluid may reference a provided model (the old `Weak` resolver could not reach those).
#[test]
fn a_reference_may_name_a_provided_model() {
    let provided = Arc::new(FluidRecord::synthetic("Base").unwrap().compile().unwrap());
    let base = Registry::empty().with_model(provided).unwrap();
    let source = Shared::new(&[(&["A"], &["base"], true)]);
    let reg = layer(&base, &source).unwrap();
    let state = gas_state(&reg, "A");
    let viscosity = reg.get("A").unwrap().prop(&state, Prop::Viscosity);
    assert_eq!(viscosity, Err(Error::NoModel { prop: Prop::Viscosity })); // resolved, materialised, M8 evaluates
    assert_eq!(source.reads(), [1]);
}

/// E6, PLAN.md M2.6: a browser fetches a pack (several v1 blobs and their index) and adds it over the embedded
/// registry with `withPack`. Its fluids resolve by any of their names and compute; a reference into the embedded layer
/// resolves when the layer is built; a name the embedded data already has, a corrupt pack and another version are
/// refused; the embedded registry value is unchanged.
#[test]
fn pack_from_generated_bytes_layers_over_embedded() {
    let fluid = |names: &[&str], requires: &[&str]| {
        let blob: Arc<[u8]> = FluidRecord::synthetic(names[0]).unwrap().encode().into();
        (names.iter().map(|n| n.to_string()).collect(), requires.iter().map(|r| r.to_string()).collect(), blob)
    };
    let bytes = phasekit_core::internal::pack(&[fluid(&["PackA", "pa-1"], &[]), fluid(&["PackB"], &["r134a"])]);
    let embedded = Registry::embedded().unwrap();
    let page = embedded.with_source(Box::new(Pack::new(bytes.clone().into()).unwrap()), DataSet::Corrected).unwrap();
    assert_eq!(page.canonical_name("PA-1"), Some("PackA"));
    gas_state(&page, "packa");
    gas_state(&page, "PackB");
    assert_eq!(page.canonical_name("R1234ZE(E)"), Some("R1234ze(E)")); // the embedded layer, shared
    assert!(embedded.get("PackA").is_err());

    let clash = phasekit_core::internal::pack(&[fluid(&["water"], &[])]);
    let refused = embedded.with_source(Box::new(Pack::new(clash.into()).unwrap()), DataSet::Corrected);
    assert_eq!(refused.unwrap_err(), Error::Load(LoadError::DuplicateName("water".into())));
    let mut corrupt = bytes.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert_eq!(Pack::new(corrupt.into()).unwrap_err(), LoadError::Format("pack checksum mismatch".into()));
    let mut v2 = bytes;
    v2[8] = 2;
    let err = Pack::new(v2.into()).unwrap_err();
    assert!(matches!(&err, LoadError::Format(m) if m.starts_with("pack version 2")), "{err:?}");
}

/// PLAN.md M4.4: every embedded fluid compiles under both datasets, and each compiled model evaluates: α^r and α⁰ to
/// order 4 at (1.05 T_r, 0.9 ρ_r) are finite. (On δ = 1 itself the non-analytic terms' order-4 δ-derivatives are
/// infinite; that line and the critical point are M4.6.)
#[test]
fn all_136_fluids_compile_under_both_datasets() {
    for set in [DataSet::Parity, DataSet::Corrected] {
        let reg = Registry::from_embedded(set).unwrap();
        let mut compiled = 0;
        for f in phasekit_data::FLUIDS {
            let fluid = reg.get(f.name).unwrap_or_else(|e| panic!("{}: {e}", f.name));
            let eos = fluid.model().helmholtz().unwrap();
            let record = phasekit_core::internal::record(&reg, f.name).unwrap();
            let (t, rho) = (1.05 * record.eos.t_reducing, 0.9 * record.eos.rho_reducing);
            let all = eos.residual(t, rho, Order::Four) + eos.ideal(t, rho, Order::Four);
            assert!((0..=4).all(|n| (0..=n).all(|i| all.get(i, n - i).is_some_and(f64::is_finite))), "{}", f.name);
            compiled += 1;
        }
        assert_eq!(compiled, 136, "{set:?}");
    }
}

/// The embedded Water entry as a `DataSource` that counts its blob reads.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
#[allow(clippy::disallowed_types, reason = "a test-only load counter; the library has no atomics (ROT-031)")]
struct RealWater(AtomicUsize);

#[cfg(not(target_family = "wasm"))]
impl DataSource for RealWater {
    fn names(&self) -> Vec<Vec<String>> {
        let water = phasekit_data::FLUIDS.iter().find(|f| f.name == "Water").unwrap();
        vec![std::iter::once(water.name).chain(water.aliases.iter().copied()).map(str::to_string).collect()]
    }
    fn blob(&self, _: FluidId) -> Result<Blob, LoadError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Blob::Static(phasekit_data::FLUIDS.iter().find(|f| f.name == "Water").unwrap().blob))
    }
    fn references(&self, _: FluidId) -> Vec<String> {
        Vec::new()
    }
}

/// PLAN.md M4.4 (D8, real data): 16 threads released together by a barrier race the first `get` of the embedded Water
/// blob; it is decoded and compiled once, and every thread gets the same model and the same bits for a state, by any
/// of Water's names.
#[cfg(not(target_family = "wasm"))]
#[test]
#[allow(clippy::disallowed_types, reason = "a test-only load counter; the library has no atomics (ROT-031)")]
fn embedded_water_initialises_once_under_16_threads() {
    let source = Arc::new(RealWater(AtomicUsize::new(0)));
    #[derive(Debug)]
    struct Shared(Arc<RealWater>);
    impl DataSource for Shared {
        fn names(&self) -> Vec<Vec<String>> {
            self.0.names()
        }
        fn blob(&self, id: FluidId) -> Result<Blob, LoadError> {
            self.0.blob(id)
        }
        fn references(&self, id: FluidId) -> Vec<String> {
            self.0.references(id)
        }
    }
    let reg = Registry::empty().with_source(Box::new(Shared(Arc::clone(&source))), DataSet::Corrected).unwrap();
    let names = ["Water", "water", "H2O", "R718"];
    let results = race(|k| {
        let fluid = reg.get(names[k % names.len()]).unwrap();
        let a = fluid.model().helmholtz().unwrap().residual(500.0, 20_000.0, Order::Two);
        (std::ptr::from_ref(fluid.model()).cast::<()>() as usize, a.get(2, 0).unwrap().to_bits())
    });
    assert_eq!(source.0.load(Ordering::SeqCst), 1);
    assert!(results.iter().all(|r| *r == results[0]), "{results:?}");
    assert_eq!(reg.loaded().collect::<Vec<_>>(), ["Water"]);
}
