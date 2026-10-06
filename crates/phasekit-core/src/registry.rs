//! L3 registry: names → fluids, lazily decoded, lock-free after first touch (D7, D8).
//!
//! A registry is an immutable stack of layers: data sources (embedded data, runtime packs, test doubles)
//! and provided models. `with_source`, `with_model` and `with_reference` return a NEW registry that shares
//! every existing layer, so nothing is decoded twice and nothing global is mutated (E6, Lean graft).
//! Each data-backed fluid is one `Arc<Slot>`: racing first requests decode once; different fluids never
//! contend; a failed decode is cached. A slot holds strong handles to the slots its lazy parts need (ECS
//! reference fluids), resolved and checked for cycles when the layer is built (E5, S-05).

use core::cmp::Ordering;
use std::sync::{Arc, OnceLock};

use crate::data::{DataSet, DataSource, FluidId, FluidRecord};
use crate::error::{Error, LoadError};
use crate::fluid::{Fluid, ReferenceState};
use crate::model::ThermoModel;
use crate::transport::{TransportSet, ViscosityModel};

/// Case-insensitive (ASCII) comparison without allocating; keys are stored lower-case.
fn cmp_folded(key: &str, query: &str) -> Ordering {
    key.bytes().cmp(query.bytes().map(|b| b.to_ascii_lowercase()))
}

/// A strong handle to a fluid another fluid's lazy parts need.
#[derive(Clone, Debug)]
enum Dep {
    /// A data-backed fluid, decoded when first needed.
    Slot(Arc<Slot>),
    /// A provided model, ready.
    Ready(Fluid),
}

impl Dep {
    fn fluid(&self) -> Result<Fluid, LoadError> {
        match self {
            Dep::Slot(slot) => slot.get().cloned(),
            Dep::Ready(fluid) => Ok(fluid.clone()),
        }
    }
}

/// One data-backed fluid: its lazily decoded package and everything needed to decode it.
#[derive(Debug)]
struct Slot {
    cell: OnceLock<Result<Fluid, LoadError>>,
    name: Box<str>,
    id: FluidId,
    source: Arc<dyn DataSource>,
    data_set: DataSet,
    /// Resolved at layer construction; the graph is acyclic, so these strong handles form no `Arc` cycle.
    deps: Box<[Dep]>,
}

impl Slot {
    fn get(&self) -> Result<&Fluid, LoadError> {
        self.cell.get_or_init(|| self.load()).as_ref().map_err(Clone::clone)
    }

    /// Cold path: decode, correct, compile. The transport closure captures the resolved reference handles
    /// (and, in the real decoder, the blob), so it needs nothing from the registry later.
    fn load(&self) -> Result<Fluid, LoadError> {
        let blob = self.source.blob(self.id)?;
        let mut record = FluidRecord::decode(blob.bytes())?;
        if record.name != *self.name {
            return Err(LoadError::Format("record name does not match its index entry".into()));
        }
        record.apply(self.data_set)?;
        let deps = self.deps.clone();
        let builder = record.builder().map_err(|e| match e {
            Error::Load(l) => l,
            other => LoadError::Format(other.to_string().into()),
        })?;
        let package = if deps.is_empty() {
            builder.build()
        } else {
            // Sketch: the toy record's viscosity is ECS against its first declared reference (M8: the blob's
            // transport section names the form and the reference).
            let ecs = move || {
                let reference = deps.first().map(Dep::fluid).transpose()?;
                Ok(TransportSet {
                    viscosity: reference.map(|reference| ViscosityModel::Ecs { reference }),
                    ..Default::default()
                })
            };
            builder.lazy_transport(ecs).build()
        };
        Ok(Fluid::new(Arc::new(package)))
    }
}

/// Fluids from one data source.
#[derive(Debug)]
struct DataLayer {
    /// Sorted lower-case names → slot index.
    keys: Box<[(Box<str>, u32)]>,
    slots: Box<[Arc<Slot>]>,
    /// Built over the embedded data: misses consult the list of not-embedded fluids.
    embedded: bool,
}

impl DataLayer {
    fn find(&self, name: &str) -> Option<&Arc<Slot>> {
        let i = self.keys.binary_search_by(|(k, _)| cmp_folded(k, name)).ok()?;
        self.keys.get(i).and_then(|(_, s)| self.slots.get(*s as usize))
    }
}

/// A model registered with `with_model`, or a re-gauged handle from `with_reference`.
#[derive(Debug)]
struct ModelLayer {
    keys: Box<[Box<str>]>,
    fluid: Fluid,
}

impl ModelLayer {
    fn matches(&self, name: &str) -> bool {
        self.keys.iter().any(|k| cmp_folded(k, name).is_eq())
    }
}

#[derive(Clone, Debug)]
enum Layer {
    Data(Arc<DataLayer>),
    Model(Arc<ModelLayer>),
}

/// Name → fluid. Cheap to clone (one `Arc`); share it by reference or clone. Never mutated.
#[derive(Clone, Debug)]
pub struct Registry {
    layers: Arc<[Layer]>,
}

#[cfg(feature = "embedded")]
#[derive(Debug)]
struct Embedded {
    fluids: Vec<&'static phasekit_data::FluidEntry>,
}

#[cfg(feature = "embedded")]
impl Embedded {
    /// Only fluids whose bytes are compiled in are indexed, so a later pack can supply the others (E6).
    fn new() -> Self {
        Self { fluids: phasekit_data::FLUIDS.iter().filter(|f| !f.blob.is_empty()).collect() }
    }

    /// The embedded layer as a build with only `names` on would have it (tests of the not-embedded path, which a
    /// build with every fluid's feature on cannot reach otherwise).
    #[cfg(test)]
    fn only(names: &[&str]) -> Self {
        Self { fluids: Self::new().fluids.into_iter().filter(|f| names.contains(&f.name)).collect() }
    }
}

#[cfg(feature = "embedded")]
impl DataSource for Embedded {
    fn names(&self) -> Vec<Vec<String>> {
        let all = |f: &&phasekit_data::FluidEntry| {
            core::iter::once(f.name).chain(f.aliases.iter().copied()).map(String::from)
        };
        self.fluids.iter().map(|f| all(f).collect()).collect()
    }
    fn blob(&self, id: FluidId) -> Result<crate::data::Blob, LoadError> {
        let f = self.fluids.get(id.0 as usize).ok_or(LoadError::Format("fluid id out of range".into()))?;
        Ok(crate::data::Blob::Static(f.blob))
    }
    fn references(&self, id: FluidId) -> Vec<String> {
        self.fluids
            .get(id.0 as usize)
            .map(|f| f.requires.iter().map(|r| String::from(*r)).collect())
            .unwrap_or_default()
    }
}

impl Registry {
    /// A registry with no fluids.
    pub fn empty() -> Registry {
        Registry { layers: Arc::new([]) }
    }

    /// The process-wide embedded registry. Built on first call from the static index; decodes nothing.
    /// Fails only if the generated index is inconsistent, which datagen and a test rule out.
    #[cfg(feature = "embedded")]
    pub fn embedded() -> Result<&'static Registry, Error> {
        static EMBEDDED: std::sync::LazyLock<Result<Registry, Error>> =
            std::sync::LazyLock::new(|| Registry::from_embedded(DataSet::Corrected));
        EMBEDDED.as_ref().map_err(Clone::clone)
    }

    /// A private registry over the embedded data, e.g. `DataSet::Parity` for oracle fixtures.
    #[cfg(feature = "embedded")]
    pub fn from_embedded(data_set: DataSet) -> Result<Registry, Error> {
        Registry::empty().push_source(Box::new(Embedded::new()), data_set, true)
    }

    /// A NEW registry that also serves the fluids of `source` (a runtime `Pack`, files, a test double).
    /// Refuses names that collide with this registry's, references that no layer provides, and reference
    /// cycles, here rather than at first use (E5). Existing layers are shared, never re-decoded (E6).
    pub fn with_source(&self, source: Box<dyn DataSource>, data_set: DataSet) -> Result<Registry, Error> {
        self.push_source(source, data_set, false)
    }

    fn push_source(&self, source: Box<dyn DataSource>, data_set: DataSet, embedded: bool) -> Result<Registry, Error> {
        let source: Arc<dyn DataSource> = Arc::from(source);
        let names = source.names();
        let mut keys: Vec<(Box<str>, u32)> = Vec::new();
        for (i, list) in (0u32..).zip(&names) {
            keys.extend(list.iter().map(|n| (n.to_ascii_lowercase().into_boxed_str(), i)));
        }
        keys.sort();
        keys.dedup();
        if let Some(w) = keys.windows(2).find(|w| w[0].0 == w[1].0) {
            return Err(LoadError::DuplicateName(w[0].0.clone()).into());
        }
        if let Some((taken, _)) = keys.iter().find(|(k, _)| self.canonical_name(k).is_some()) {
            return Err(LoadError::DuplicateName(taken.clone()).into());
        }
        let canonical: Vec<Box<str>> =
            names.iter().map(|l| l.first().map_or_else(Box::default, |n| n.as_str().into())).collect();
        let refs: Vec<Vec<String>> = (0u32..).zip(&names).map(|(i, _)| source.references(FluidId(i))).collect();
        // Build slots in dependency order: a slot exists only after the slots it needs. No progress with
        // slots left means a cycle (a cycle would otherwise deadlock on re-entrant `OnceLock` init).
        let mut slots: Vec<Option<Arc<Slot>>> = vec![None; names.len()];
        let mut progress = true;
        while progress {
            progress = false;
            for i in 0..names.len() {
                if slots[i].is_some() {
                    continue;
                }
                let mut deps = Vec::new();
                for r in &refs[i] {
                    let local = keys.binary_search_by(|(k, _)| cmp_folded(k, r)).ok().map(|j| keys[j].1 as usize);
                    let dep = match local {
                        Some(j) => slots[j].clone().map(Dep::Slot),
                        None => Some(self.dep(r).ok_or_else(|| LoadError::MissingReference {
                            fluid: canonical[i].clone(),
                            reference: r.as_str().into(),
                        })?),
                    };
                    deps.extend(dep);
                }
                if deps.len() == refs[i].len() {
                    let (name, id) = (canonical[i].clone(), FluidId(i as u32));
                    let deps = deps.into_boxed_slice();
                    let slot = Slot { cell: OnceLock::new(), name, id, source: Arc::clone(&source), data_set, deps };
                    slots[i] = Some(Arc::new(slot));
                    progress = true;
                }
            }
        }
        if let Some(stuck) = slots.iter().position(Option::is_none) {
            return Err(LoadError::ReferenceCycle(canonical[stuck].clone()).into());
        }
        let slots: Box<[Arc<Slot>]> = slots.into_iter().flatten().collect();
        Ok(self.push(Layer::Data(Arc::new(DataLayer { keys: keys.into(), slots, embedded }))))
    }

    /// A NEW registry that also serves `model` under its name and aliases. Third-party families reach
    /// `get`, compat strings, batch, C and WASM this way, with no core edit.
    pub fn with_model(&self, model: Arc<dyn ThermoModel>) -> Result<Registry, Error> {
        let info = model.info();
        let keys: Box<[Box<str>]> =
            core::iter::once(info.name()).chain(info.aliases()).map(|n| n.to_ascii_lowercase().into()).collect();
        if let Some(taken) = keys.iter().find(|k| self.canonical_name(k).is_some()) {
            return Err(LoadError::DuplicateName(taken.clone()).into());
        }
        Ok(self.push(Layer::Model(Arc::new(ModelLayer { keys, fluid: Fluid::new(model) }))))
    }

    /// A NEW registry in which `name` (and every alias of that fluid) reports in `reference`: the value
    /// form of CoolProp's `set_reference_stateS`, reachable from compat strings, C and JS (E10).
    pub fn with_reference(&self, name: &str, reference: ReferenceState) -> Result<Registry, Error> {
        let fluid = self.get(name)?.with_reference(reference)?;
        let keys = self.keys_of(name);
        Ok(self.push(Layer::Model(Arc::new(ModelLayer { keys, fluid }))))
    }

    /// The fluid answering to `name` (canonical, alias or CAS; ASCII case-insensitive). Returns a reference:
    /// no refcount traffic per lookup. First use of a data-backed fluid decodes it once. Newest layer wins,
    /// which only matters for `with_reference` (other layers never share names).
    pub fn get(&self, name: &str) -> Result<&Fluid, Error> {
        for layer in self.layers.iter().rev() {
            match layer {
                Layer::Data(d) => {
                    if let Some(slot) = d.find(name) {
                        return slot.get().map_err(Error::Load);
                    }
                }
                Layer::Model(m) if m.matches(name) => return Ok(&m.fluid),
                Layer::Model(_) => {}
            }
        }
        Err(self.missing(name).into())
    }

    /// The canonical name for `name`, without loading anything.
    pub fn canonical_name(&self, name: &str) -> Option<&str> {
        self.layers.iter().rev().find_map(|layer| match layer {
            Layer::Data(d) => d.find(name).map(|s| &*s.name),
            Layer::Model(m) => m.matches(name).then(|| m.fluid.info().name()),
        })
    }

    /// Decodes the named fluids now (servers warming up).
    pub fn preload(&self, names: &[&str]) -> Result<(), Error> {
        names.iter().try_for_each(|n| self.get(n).map(|_| ()))
    }

    /// Canonical names of fluids that are materialised (for memory introspection and lazy-load tests).
    pub fn loaded(&self) -> impl Iterator<Item = &str> {
        self.layers.iter().flat_map(|layer| -> Box<dyn Iterator<Item = &str> + '_> {
            match layer {
                Layer::Data(d) => {
                    Box::new(d.slots.iter().filter(|s| matches!(s.cell.get(), Some(Ok(_)))).map(|s| &*s.name))
                }
                Layer::Model(m) => Box::new(core::iter::once(m.fluid.info().name())),
            }
        })
    }

    /// The decoded, uncorrected record of a data-backed fluid (arbitration and register tests; reached
    /// through the semver-exempt `phasekit_core::internal::record`).
    pub(crate) fn record(&self, name: &str) -> Result<FluidRecord, Error> {
        let slot = self.layers.iter().rev().find_map(|l| match l {
            Layer::Data(d) => d.find(name),
            Layer::Model(_) => None,
        });
        let slot = slot.ok_or_else(|| LoadError::UnknownName(name.into()))?;
        Ok(FluidRecord::decode(slot.source.blob(slot.id)?.bytes())?)
    }

    fn push(&self, layer: Layer) -> Registry {
        Registry { layers: self.layers.iter().cloned().chain(core::iter::once(layer)).collect() }
    }

    /// A strong handle to the fluid called `name` in an existing layer.
    fn dep(&self, name: &str) -> Option<Dep> {
        self.layers.iter().rev().find_map(|layer| match layer {
            Layer::Data(d) => d.find(name).map(|s| Dep::Slot(Arc::clone(s))),
            Layer::Model(m) => m.matches(name).then(|| Dep::Ready(m.fluid.clone())),
        })
    }

    /// Every key that resolves to the same fluid as `name` in its layer.
    fn keys_of(&self, name: &str) -> Box<[Box<str>]> {
        let keys = self.layers.iter().rev().find_map(|layer| match layer {
            Layer::Data(d) => d.find(name).map(|slot| {
                let same =
                    d.keys.iter().filter(|(_, i)| d.slots.get(*i as usize).is_some_and(|s| Arc::ptr_eq(s, slot)));
                same.map(|(k, _)| k.clone()).collect()
            }),
            Layer::Model(m) => m.matches(name).then(|| m.keys.clone()),
        });
        keys.unwrap_or_default()
    }

    /// Why `name` is not here. Not cached: a later layer may still supply it (E6).
    fn missing(&self, name: &str) -> LoadError {
        let has_embedded = self.layers.iter().any(|l| matches!(l, Layer::Data(d) if d.embedded));
        match not_embedded(name).filter(|_| has_embedded) {
            Some(e) => e,
            None => LoadError::UnknownName(name.into()),
        }
    }
}

/// `NotEmbedded { feature }` for a CoolProp fluid whose Cargo feature is off.
#[cfg(feature = "embedded")]
fn not_embedded(name: &str) -> Option<LoadError> {
    let names = |f: &&phasekit_data::FluidEntry| core::iter::once(f.name).chain(f.aliases.iter().copied());
    let f = phasekit_data::FLUIDS.iter().find(|f| names(f).any(|n| n.eq_ignore_ascii_case(name)))?;
    Some(LoadError::NotEmbedded { name: f.name.into(), feature: f.feature })
}

#[cfg(not(feature = "embedded"))]
fn not_embedded(_name: &str) -> Option<LoadError> {
    None
}

#[cfg(all(test, feature = "embedded"))]
mod tests {
    use super::*;
    use crate::error::Error;

    /// The real index (M2.5): names, aliases, CAS numbers and InChIKeys resolve, case-insensitively, without
    /// decoding anything. CoolProp's canonical name for propane is `n-Propane`.
    #[test]
    fn embedded_index_builds_and_resolves_without_loading() {
        let reg = Registry::embedded().unwrap();
        assert_eq!(reg.canonical_name("h2o"), Some("Water"));
        assert_eq!(reg.canonical_name("7732-18-5"), Some("Water"));
        assert_eq!(reg.canonical_name("n-PROPANE"), Some("n-Propane"));
        assert_eq!(reg.canonical_name("r290"), Some("n-Propane"));
        assert_eq!(reg.canonical_name("ATUOYWHBWRKTHZ-UHFFFAOYSA-N"), Some("n-Propane"));
        assert_eq!(reg.canonical_name("R1234ze(E)"), Some("R1234ze(E)"));
        assert_eq!(reg.canonical_name("unobtainium"), None);
        assert_eq!(reg.loaded().count(), 0);
    }

    /// A source that counts blob reads per fluid.
    #[derive(Debug)]
    #[allow(clippy::disallowed_types, reason = "a test-only read counter; the library has no atomics (ROT-031)")]
    struct Counting<S>(S, Arc<Vec<std::sync::atomic::AtomicUsize>>);

    #[allow(clippy::disallowed_types, reason = "a test-only read counter; the library has no atomics (ROT-031)")]
    impl<S: DataSource> Counting<S> {
        fn new(source: S) -> (Self, Arc<Vec<std::sync::atomic::AtomicUsize>>) {
            let reads = Arc::new(source.names().iter().map(|_| std::sync::atomic::AtomicUsize::new(0)).collect());
            (Counting(source, Arc::clone(&reads)), reads)
        }
    }

    impl<S: DataSource> DataSource for Counting<S> {
        fn names(&self) -> Vec<Vec<String>> {
            self.0.names()
        }
        fn blob(&self, id: FluidId) -> Result<crate::data::Blob, LoadError> {
            if let Some(n) = self.1.get(id.0 as usize) {
                n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            self.0.blob(id)
        }
        fn references(&self, id: FluidId) -> Vec<String> {
            self.0.references(id)
        }
    }

    #[allow(clippy::disallowed_types, reason = "a test-only read counter; the library has no atomics (ROT-031)")]
    fn total(reads: &[std::sync::atomic::AtomicUsize]) -> usize {
        reads.iter().map(|r| r.load(std::sync::atomic::Ordering::SeqCst)).sum()
    }

    /// Per-fluid read counts.
    #[allow(clippy::disallowed_types, reason = "a test-only read counter; the library has no atomics (ROT-031)")]
    type Reads = Arc<Vec<std::sync::atomic::AtomicUsize>>;

    fn embedded_layer(source: Embedded) -> Result<(Registry, Reads), Error> {
        let (counting, reads) = Counting::new(source);
        Ok((Registry::empty().push_source(Box::new(counting), DataSet::Corrected, true)?, reads))
    }

    /// ROT-027, PLAN.md M2.6: building the embedded layer indexes all 136 fluids and every one of their 556 keys
    /// without reading a single blob.
    #[test]
    fn embedded_layer_indexes_all_136_without_decoding() {
        let (reg, reads) = embedded_layer(Embedded::new()).unwrap();
        assert_eq!(reads.len(), 136);
        for f in phasekit_data::FLUIDS {
            for key in core::iter::once(f.name).chain(f.aliases.iter().copied()) {
                assert_eq!(reg.canonical_name(&key.to_ascii_uppercase()), Some(f.name), "{key}");
            }
        }
        assert_eq!((total(&reads), reg.loaded().count()), (0, 0));
        let (counting, _) = Counting::new(Embedded::new());
        let r143a = counting.names().iter().position(|n| n[0] == "R143a").unwrap();
        assert_eq!(counting.references(FluidId(r143a as u32)), ["R134a"]); // declared, resolved, not read
    }

    /// ROT-039, PLAN.md M2.6: a fluid whose term kinds have not landed fails with a typed error naming the step that
    /// lands them, read once and cached, never a partial model; other fluids are untouched.
    #[test]
    fn unimplemented_kinds_are_cached_typed_load_errors() {
        let (reg, reads) = embedded_layer(Embedded::new()).unwrap();
        let first = reg.get("Water").unwrap_err();
        assert_eq!(first, Error::Load(LoadError::Format("Gaussian terms land at M3.4".into())));
        assert_eq!(reg.get("water").unwrap_err(), first);
        assert_eq!(total(&reads), 1);
        assert_eq!(reg.loaded().count(), 0);
    }

    /// E6, PLAN.md M2.6: a known CoolProp fluid whose feature is off is `NotEmbedded { feature }`, never cached, so
    /// a pack added later supplies it; the registry without the pack is unchanged.
    #[test]
    fn not_embedded_is_uncached() {
        let (reg, _) = embedded_layer(Embedded::only(&["Water"])).unwrap();
        let missing = Error::Load(LoadError::NotEmbedded { name: "R134a".into(), feature: "fluid-r134a" });
        assert_eq!(reg.get("r134a").unwrap_err(), missing);
        assert_eq!(reg.get("R134A").unwrap_err(), missing);
        assert_eq!(reg.get("unobtainium").unwrap_err(), Error::Load(LoadError::UnknownName("unobtainium".into())));
        let blob: Arc<[u8]> = FluidRecord::synthetic("R134a").unwrap().encode().into();
        let pack = crate::internal::pack(&[(vec!["R134a".into()], vec![], blob)]);
        let page = reg.with_source(Box::new(crate::data::Pack::new(pack.into()).unwrap()), DataSet::Corrected).unwrap();
        assert_eq!(page.get("r134a").unwrap().info().name(), "R134a");
        assert_eq!(reg.get("r134a").unwrap_err(), missing);
    }

    /// The number of embedded fluids that compile, under both datasets, never drops (PLAN.md M2.6). Raised as kinds
    /// land; 136 at M4.4. At M2.6, 26 fluids hold only kinds the evaluator has (Power terms; Lead, LogTau, Power and
    /// Planck-Einstein ideal terms).
    const MIN_COMPILABLE: usize = 26;

    #[test]
    fn compilable_fluid_count_never_drops() {
        let count = |set: DataSet| {
            let reg = Registry::from_embedded(set).unwrap();
            phasekit_data::FLUIDS.iter().filter(|f| reg.get(f.name).is_ok()).count()
        };
        let (parity, corrected) = (count(DataSet::Parity), count(DataSet::Corrected));
        assert_eq!(parity, corrected);
        assert!(parity >= MIN_COMPILABLE, "{parity} compile, fewer than {MIN_COMPILABLE}");
    }

    /// PLAN.md M2.11: until M5.2a computes them, every shipped blob carries the caloric section empty, and asking for
    /// the curves is the typed "not yet" error.
    #[test]
    fn caloric_section_is_empty_until_m5_2a() {
        for f in phasekit_data::FLUIDS {
            let record = FluidRecord::decode(f.blob).unwrap();
            assert_eq!(record.caloric, None, "{}", f.name);
            assert_eq!(record.caloric_curves(), Err(LoadError::Format("caloric curves land at M5.2a".into())));
        }
        assert_eq!(phasekit_data::FLUIDS.len(), 136);
    }

    /// Map 09 §4.5: the shipped index has 556 ASCII case-folded keys over 136 fluids, none shared between fluids;
    /// building the embedded registry refuses a collision, so `embedded()` succeeding proves it at run time too.
    #[test]
    fn index_has_556_keys_and_no_collisions() {
        let fluids = phasekit_data::FLUIDS;
        let mut keys: Vec<(String, &str)> = fluids
            .iter()
            .flat_map(|f| {
                core::iter::once(f.name).chain(f.aliases.iter().copied()).map(|k| (k.to_ascii_lowercase(), f.name))
            })
            .collect();
        keys.sort();
        let total = keys.len();
        keys.dedup();
        assert_eq!((fluids.len(), total, keys.len()), (136, 556, 556), "each key listed once");
        assert!(keys.windows(2).all(|w| w[0].0 != w[1].0), "no key names two fluids");
        assert!(fluids.windows(2).all(|w| w[0].name < w[1].name), "sorted by name");
        assert!(Registry::embedded().is_ok());
    }

    /// CoolProp lists "water" and "WATER" (and "H2O" and "h2o") as separate aliases; the index holds each key once,
    /// and every spelling resolves.
    #[test]
    fn case_variants_collapse() {
        let water = phasekit_data::FLUIDS.iter().find(|f| f.name == "Water").unwrap();
        assert_eq!(water.aliases, ["H2O", "R718", "7732-18-5", "XLYOFNOQVPJJNP-UHFFFAOYSA-N"]);
        let reg = Registry::embedded().unwrap();
        for spelling in ["water", "WATER", "Water", "wAtEr", "H2O", "h2o", "r718"] {
            assert_eq!(reg.canonical_name(spelling), Some("Water"), "{spelling}");
        }
    }
}
