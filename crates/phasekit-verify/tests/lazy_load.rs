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
    Blob, DataSet, DataSource, Density, Error, FlashOptions, FluidId, Input, LoadError, Pack, Phase, Prop, Registry,
    Temperature,
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
    std::thread::scope(|s| {
        for _ in 0..16 {
            s.spawn(|| reg.get("Beta").map(|_| ()));
        }
    });
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
    std::thread::scope(|s| {
        for _ in 0..16 {
            s.spawn(|| reg.get("A").unwrap().prop(&state, Prop::Viscosity));
        }
    });
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
