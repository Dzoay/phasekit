//! Mixture names (PLAN.md M13.13): CoolProp's mixture strings, read by a registry resolver ([`with_mixtures`]), and
//! its predefined mixtures by name ([`predefined`]).
//!
//! The grammar, nothing read leniently (ROT-166): an optional `HEOS::` or `GERG2008::`, then `A[x]&B[y]&...`, each
//! name a registry name, alias or CAS number and each x a plain decimal (digits, a point, an exponent); without
//! `GERG2008::`, a predefined mixture's `name.mix` (ASCII case-insensitive, as registry names are); with it, a GERG-2008
//! component's name alone, its pure fluid. The fractions are validated as [`crate::mixture`]'s (ROT-193) and their
//! zeros trimmed (ROT-192): a name that trims to one component is that fluid.

use std::sync::Arc;

use phasekit_core::{
    Capabilities, CriticalPoint, DataSet, Error, FlashOptions, Fluid, FluidInfo, HelmholtzModel, Limits, LoadError,
    NativeInput, Order, PointDerivs, Registry, Resolver, State, ThermoModel,
};

use crate::data::PredefinedData;
use crate::generated::{PAIRS, PREDEFINED};
use crate::mixture::{gerg2008_model, trimmed};
use crate::{Mixture, MultiFluid, gerg};

/// A NEW registry whose [`Registry::resolve`] also reads mixture names (the module's grammar), on `data_set`'s
/// library: `HEOS::R32[0.697615]&R125[0.302385]` and `R410A.mix` as CoolProp 8.0.0 reads them,
/// `GERG2008::Methane[0.9]&Ethane[0.1]` on GERG-2008 (user decision MX6). The names `registry` holds, the pseudo-pure
/// blends' among them, keep their fluids (user decision 4). A mixture name builds its model when first resolved, and
/// the resolver keeps the models of the last [`CACHED`] names (user decision MX9), so that a name used again, as
/// `PropsSI` calls use it, finds its model and the critical points it has computed. It serves the registries made from
/// the new one, which hold the same fluids under the same names (a layer only adds names or re-gauges one).
pub fn with_mixtures(registry: &Registry, data_set: DataSet) -> Registry {
    registry.with_resolver(Arc::new(Names { data_set, built: cache::Cache::new(CACHED) }))
}

/// How many names' models a resolver keeps (user decision MX9), the least recently used dropped first.
pub const CACHED: usize = 64;

/// CoolProp 8.0.0's predefined mixture `name` (`R410A`; ASCII case-insensitive) over `registry`'s fluids and
/// `data_set`'s library, at the mole fractions `data_set` gives it (ROT-199, DIV-0064): `Parity` v8.0.0's stored ones;
/// otherwise, where ASHRAE 34 defines the blend, those its mass fractions give with the registry's molar masses. A name
/// v8.0.0 does not define is `UnknownName("<name>.mix")`; a blend with a pair the library lacks is refused as
/// [`MultiFluid::new`] refuses it (ROT-200).
pub fn predefined(registry: &Registry, name: &str, data_set: DataSet) -> Result<Fluid, Error> {
    predefined_model(registry, name, data_set).map(Fluid::new)
}

/// The names of CoolProp 8.0.0's predefined mixtures (`R410A`, without `.mix`), each once, in byte order.
pub fn predefined_names() -> impl Iterator<Item = &'static str> {
    PREDEFINED.iter().map(|m| m.name)
}

/// The library's binary pairs, by their components' CAS numbers sorted as strings, in that order: v8.0.0's.
pub fn binary_pairs() -> impl Iterator<Item = [&'static str; 2]> {
    PAIRS.iter().map(|p| p.cas)
}

/// The components of mixture name `name` (the module's grammar), so that a caller can make a registry serve them before
/// it resolves the name (the site fetches their packs; PLAN.md M21.5): each as the name writes it, or, for a predefined
/// mixture, as the library lists it (CoolProp's names, `R32`, `NITROGEN`), with any zero fraction kept. `None` where
/// `name` is no mixture name; where it breaks the grammar or names no predefined mixture, the refusal resolving it gives.
pub fn components(name: &str) -> Option<Result<Vec<&str>, Error>> {
    Some(read(name)?.and_then(|read| match read {
        Name::Library(parts) | Name::Gerg(parts) => Ok(parts.into_iter().map(|(c, _)| c).collect()),
        Name::Predefined(stem) => predefined_data(stem).map(|data| data.components.to_vec()),
    }))
}

/// The resolver [`with_mixtures`] registers: the data set its names are read on, and the models it built.
#[derive(Debug)]
struct Names {
    data_set: DataSet,
    built: cache::Cache,
}

impl Resolver for Names {
    /// A mixture name's model: the one built for it before, while kept, else a new one, kept. A refusal is not kept.
    fn resolve(&self, name: &str, registry: &Registry) -> Option<Result<Arc<dyn ThermoModel>, Error>> {
        let read = read(name)?;
        if let Some(model) = self.built.get(name) {
            return Some(Ok(model));
        }
        let model = match read {
            Ok(Name::Library(parts)) => library(registry, &parts, self.data_set),
            Ok(Name::Gerg(parts)) => gerg2008_model(registry, &parts),
            Ok(Name::Predefined(stem)) => predefined_model(registry, stem, self.data_set),
            Err(e) => Err(e),
        };
        Some(model.inspect(|m| self.built.put(name, m)))
    }
}

/// The models a resolver built, by name (user decision MX9): the library's one lock outside the C ABI, the exception
/// to D8 that decision grants. A model is immutable, so one found here is the one a fresh build would give, bit for bit.
mod cache {
    #![allow(clippy::disallowed_types, reason = "the mixture names' cache, one lock (user decision MX9; D8)")]

    use std::sync::{Arc, Mutex, PoisonError};

    use phasekit_core::ThermoModel;

    /// A name and the model built for it.
    type Entry = (Box<str>, Arc<dyn ThermoModel>);

    /// The models of the last `capacity` names, the most recently used first.
    #[derive(Debug)]
    pub(super) struct Cache {
        capacity: usize,
        entries: Mutex<Vec<Entry>>,
    }

    impl Cache {
        /// Empty, for `capacity` names.
        pub(super) fn new(capacity: usize) -> Cache {
            Cache { capacity, entries: Mutex::new(Vec::with_capacity(capacity)) }
        }

        /// The model kept for `name`, now the most recently used. A lock a panic poisoned still holds whole entries.
        pub(super) fn get(&self, name: &str) -> Option<Arc<dyn ThermoModel>> {
            let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            let at = entries.iter().position(|(n, _)| **n == *name)?;
            let entry = entries.remove(at);
            let model = Arc::clone(&entry.1);
            entries.insert(0, entry);
            Some(model)
        }

        /// Keeps `model` for `name` as the most recently used, dropping the least recently used past the capacity.
        pub(super) fn put(&self, name: &str, model: &Arc<dyn ThermoModel>) {
            let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            entries.retain(|(n, _)| **n != *name);
            entries.insert(0, (name.into(), Arc::clone(model)));
            entries.truncate(self.capacity);
        }

        /// The names kept, the most recently used first.
        #[cfg(test)]
        pub(super) fn names(&self) -> Vec<String> {
            let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
            entries.iter().map(|(n, _)| n.to_string()).collect()
        }
    }
}

/// What a mixture name reads as.
#[derive(Debug, PartialEq)]
enum Name<'a> {
    /// `A[x]&B[y]...` on the library's model, with no prefix or `HEOS::`.
    Library(Vec<(&'a str, f64)>),
    /// `GERG2008::A[x]&B[y]...`, or `GERG2008::A` at 1.
    Gerg(Vec<(&'a str, f64)>),
    /// `name.mix`: the predefined mixture's name.
    Predefined(&'a str),
}

/// `name` read as a mixture name: None where it is none (after its prefix no `&`, `[` or `]`, no `.mix` suffix, and no
/// `GERG2008::`), so that the registry's own refusal stands; `LoadError::Format`, naming it, where it breaks the grammar.
fn read(name: &str) -> Option<Result<Name<'_>, Error>> {
    let (gerg, body) = match name.strip_prefix(gerg::PREFIX) {
        Some(body) => (true, body),
        None => (false, name.strip_prefix("HEOS::").unwrap_or(name)),
    };
    if !body.contains(['&', '[', ']']) {
        return if gerg {
            Some(component(name, body).map(|c| Name::Gerg(vec![(c, 1.0)])))
        } else {
            predefined_stem(body).map(|stem| Ok(Name::Predefined(stem)))
        };
    }
    let parts = body.split('&').map(|part| fraction(name, part)).collect::<Result<Vec<_>, _>>();
    Some(parts.map(|parts| if gerg { Name::Gerg(parts) } else { Name::Library(parts) }))
}

/// The stem of a `.mix` name (its suffix ASCII case-insensitive), if `body` is one.
fn predefined_stem(body: &str) -> Option<&str> {
    let split = body.len().checked_sub(".mix".len()).filter(|&at| body.is_char_boundary(at))?;
    let (stem, suffix) = body.split_at(split);
    (!stem.is_empty() && suffix.eq_ignore_ascii_case(".mix")).then_some(stem)
}

/// One `A[x]` of mixture name `name`: the component's name and its fraction.
fn fraction<'a>(name: &str, part: &'a str) -> Result<(&'a str, f64), Error> {
    let Some((c, rest)) = part.split_once('[') else {
        return Err(malformed(name, &format!("`{part}` has no fraction in brackets")));
    };
    let Some(x) = rest.strip_suffix(']') else {
        return Err(malformed(name, &format!("`{part}` does not end with `]`")));
    };
    let decimal = !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b));
    match (component(name, c)?, decimal.then(|| x.parse::<f64>())) {
        (c, Some(Ok(x))) => Ok((c, x)),
        _ => Err(malformed(name, &format!("the fraction `{x}` is not a decimal number"))),
    }
}

/// A component's name in mixture name `name`: not empty, and none of `[`, `]`, `&` or `::` in it.
fn component<'a>(name: &str, c: &'a str) -> Result<&'a str, Error> {
    if c.is_empty() || c.contains(['[', ']', '&']) || c.contains("::") {
        return Err(malformed(name, &format!("`{c}` is not a component's name")));
    }
    Ok(c)
}

/// The refusal of a malformed mixture name.
fn malformed(name: &str, why: &str) -> Error {
    Error::Load(LoadError::Format(format!("mixture name `{name}`: {why}").into()))
}

/// The library's model of `parts`, trimmed and validated: two or more components a [`Mixture`], one that fluid
/// ([`Alone`]).
fn library(registry: &Registry, parts: &[(&str, f64)], data_set: DataSet) -> Result<Arc<dyn ThermoModel>, Error> {
    let (names, fractions) = trimmed(parts)?;
    if let [one] = names[..] {
        return Ok(Arc::new(Alone(registry.resolve(one)?)));
    }
    Ok(Arc::new(Mixture::new(MultiFluid::new(registry, &names, data_set)?, &fractions)?))
}

/// [`predefined`]'s model.
fn predefined_model(registry: &Registry, name: &str, data_set: DataSet) -> Result<Arc<dyn ThermoModel>, Error> {
    let data = predefined_data(name)?;
    let fractions = match (data_set, data.mass_fractions) {
        (DataSet::Parity, _) | (_, None) => data.mole_fractions.to_vec(),
        (_, Some(w)) => {
            let masses = data.components.iter().map(|c| Ok(registry.resolve(c)?.info().molar_mass()));
            let moles = w.iter().zip(masses).map(|(w, m)| m.map(|m| w / m)).collect::<Result<Vec<f64>, Error>>()?;
            let total: f64 = moles.iter().sum();
            moles.iter().map(|n| n / total).collect()
        }
    };
    let parts: Vec<(&str, f64)> = data.components.iter().copied().zip(fractions).collect();
    library(registry, &parts, data_set)
}

/// The library's predefined mixture `name` (ASCII case-insensitive), else `UnknownName("<name>.mix")`.
fn predefined_data(name: &str) -> Result<&'static PredefinedData, Error> {
    let unknown = || Error::Load(LoadError::UnknownName(format!("{name}.mix").into()));
    PREDEFINED.iter().find(|m| m.name.eq_ignore_ascii_case(name)).ok_or_else(unknown)
}

/// A mixture name that trims to one component (`R32[1.0]&R125[0.0]`, `Water[1.0]`): that component's fluid, each call
/// passed to its model, so that its values are the pure fluid's bit for bit (ROT-099, ROT-192), at the model's native
/// gauge as every resolved name is.
#[derive(Debug)]
struct Alone(Fluid);

impl ThermoModel for Alone {
    fn info(&self) -> &FluidInfo {
        self.0.model().info()
    }
    fn capabilities(&self) -> Capabilities {
        self.0.model().capabilities()
    }
    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        self.0.model().flash(input, opts)
    }
    fn critical_point(&self) -> Option<CriticalPoint> {
        self.0.model().critical_point()
    }
    fn limits(&self) -> Option<&Limits> {
        self.0.model().limits()
    }
    fn helmholtz(&self) -> Option<&dyn HelmholtzModel> {
        self.0.model().helmholtz()
    }
    fn derivs(&self, state: &State, order: Order) -> Option<PointDerivs> {
        self.0.model().derivs(state, order)
    }
    fn viscosity(&self, state: &State) -> Result<f64, Error> {
        self.0.model().viscosity(state)
    }
    fn conductivity(&self, state: &State) -> Result<f64, Error> {
        self.0.model().conductivity(state)
    }
    fn surface_tension(&self, t: f64) -> Result<f64, Error> {
        self.0.model().surface_tension(t)
    }
    fn melting_pressure(&self, t: f64) -> Result<f64, Error> {
        self.0.model().melting_pressure(t)
    }
    fn melting_temperature(&self, p: f64) -> Result<f64, Error> {
        self.0.model().melting_temperature(p)
    }
}

#[cfg(test)]
mod tests {
    use phasekit_core::{Basis, Input, Pair};

    use super::*;
    use crate::generated::PREDEFINED;

    fn registry(set: DataSet) -> Registry {
        Registry::from_embedded(set).unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// A name as the grammar reads it: a composition on the library's model or on GERG-2008's, a predefined mixture's
    /// name (its suffix in any case), no mixture name at all (the registry's own refusal stands, and a name whose last
    /// four bytes split a character is one), or malformed, naming the string.
    #[test]
    fn names_read_by_the_grammar() {
        let library = |parts: &[(&'static str, f64)]| Some(Ok(Name::Library(parts.to_vec())));
        assert_eq!(read("R32[0.5]&R125[0.5]"), library(&[("R32", 0.5), ("R125", 0.5)]));
        assert_eq!(read("HEOS::R32[.5]&R125[5e-1]"), library(&[("R32", 0.5), ("R125", 0.5)]));
        assert_eq!(read("Water[1]"), library(&[("Water", 1.0)]));
        let gerg = Some(Ok(Name::Gerg(vec![("Methane", 0.9), ("Ethane", 0.1)])));
        assert_eq!(read("GERG2008::Methane[0.9]&Ethane[0.1]"), gerg);
        assert_eq!(read("GERG2008::Methane"), Some(Ok(Name::Gerg(vec![("Methane", 1.0)]))));
        assert_eq!(read("R410A.mix"), Some(Ok(Name::Predefined("R410A"))));
        assert_eq!(read("HEOS::r410a.MIX"), Some(Ok(Name::Predefined("r410a"))));
        for none in ["Water", "HEOS::Water", ".mix", "HEOS::.mix", "mix", "R410A.mi", "R410A.mixx", "\u{e9}aaa"] {
            assert_eq!(read(none), None, "{none}");
        }
        let malformed = [
            "R3]2[0.5]&R125[0.5]",
            "R32[0.5]&R125[0.5]x",
            "R32[+-]&R125[1]",
            "R32[0.5e]&R125[0.5]",
            "GERG2008::",
            "GERG2008::A&B",
            "GERG2008::HEOS::Methane",
        ];
        for name in malformed {
            let format = matches!(read(name), Some(Err(Error::Load(LoadError::Format(ref m)))) if m.contains(name));
            assert!(format, "{name}: {:?}", read(name));
        }
    }

    /// A name that trims to one component is that fluid: `Water[1.0]` and `R32[1.0]&R125[0.0]` answer each of the
    /// model's calls as the registry's Water and R32 do, bit for bit; `GERG2008::Methane` is GERG-2008's pure methane,
    /// as [`crate::gerg2008`] builds it.
    #[test]
    fn a_name_of_one_component_is_that_fluid() {
        let plain = registry(DataSet::Corrected);
        let names = with_mixtures(&plain, DataSet::Corrected);
        for (name, pure, (t, rho)) in
            [("Water[1.0]", "Water", (300.0, 56_000.0)), ("R32[1.0]&R125[0.0]", "R32", (250.0, 20.0))]
        {
            let alone = names.resolve(name).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let fluid = plain.get(pure).unwrap_or_else(|e| panic!("{pure}: {e:?}"));
            let (a, b) = (alone.model(), fluid.model());
            assert_eq!(
                (a.info(), a.capabilities(), a.critical_point(), a.limits()),
                (b.info(), b.capabilities(), b.critical_point(), b.limits())
            );
            assert_eq!(a.helmholtz().and_then(|h| h.reducing()), b.helmholtz().and_then(|h| h.reducing()));
            let input = Input::new(Pair::DT, rho, t, Basis::Molar).unwrap_or_else(|e| panic!("{e:?}"));
            let state = alone.flash(input, &FlashOptions::new()).unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(Ok(state), fluid.flash(input, &FlashOptions::new()));
            // Debug strings compare bits, the slots order 3 leaves NaN included.
            let derivs = |m: &dyn ThermoModel| m.derivs(&state, Order::Three).map(|d| format!("{d:?}"));
            assert!(derivs(a).is_some() && derivs(a) == derivs(b), "{name}: {:?} against {:?}", derivs(a), derivs(b));
            assert_eq!((a.viscosity(&state), a.conductivity(&state)), (b.viscosity(&state), b.conductivity(&state)));
            assert_eq!(a.surface_tension(t), b.surface_tension(t));
            assert_eq!(
                (a.melting_pressure(t), a.melting_temperature(1e8)),
                (b.melting_pressure(t), b.melting_temperature(1e8))
            );
        }
        let methane = names.resolve("GERG2008::Methane").unwrap_or_else(|e| panic!("{e:?}"));
        let gerg = crate::gerg2008(&plain, &[("Methane", 1.0)]).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(methane.info(), gerg.info());
    }

    /// User decision MX9: a resolver keeps the models of the names it resolved. The same name again is the same model,
    /// with the critical points it computed, and gives a fresh resolver's values bit for bit; what is not a mixture
    /// name, a malformed one and a refusal are not kept.
    #[test]
    fn a_repeated_name_finds_its_model() {
        let plain = registry(DataSet::Corrected);
        let names = with_mixtures(&plain, DataSet::Corrected);
        let resolve = |registry: &Registry| registry.resolve("R32[0.5]&R125[0.5]").unwrap_or_else(|e| panic!("{e:?}"));
        let (first, again, fresh) =
            (resolve(&names), resolve(&names), resolve(&with_mixtures(&plain, DataSet::Corrected)));
        assert!(std::ptr::addr_eq(first.model(), again.model()) && !std::ptr::addr_eq(first.model(), fresh.model()));
        let input = Input::new(Pair::PT, 1e5, 300.0, Basis::Molar).unwrap_or_else(|e| panic!("{e:?}"));
        let opts = FlashOptions::new();
        assert_eq!(again.flash(input, &opts), fresh.flash(input, &opts));
        let resolver = Names { data_set: DataSet::Corrected, built: cache::Cache::new(CACHED) };
        for name in ["Water", "R32[0,5]&R125[0.5]", "R32[0.7]&R125[0.2]", "R32[0.5]&R125[0.5]"] {
            let _ = resolver.resolve(name, &plain);
        }
        assert_eq!(resolver.built.names(), ["R32[0.5]&R125[0.5]"]);
    }

    /// The cache keeps the last names put or found, the most recently used first: past its capacity the least recently
    /// used is dropped, and a name put again moves to the front, once.
    #[test]
    fn the_cache_keeps_the_last_names() {
        let water = registry(DataSet::Corrected).get("Water").cloned().unwrap_or_else(|e| panic!("{e:?}"));
        let model: Arc<dyn ThermoModel> = Arc::new(Alone(water));
        let cache = cache::Cache::new(2);
        cache.put("a", &model);
        cache.put("b", &model);
        assert_eq!(cache.names(), ["b", "a"]);
        assert!(cache.get("a").is_some_and(|m| Arc::ptr_eq(&m, &model)) && cache.names() == ["a", "b"]);
        cache.put("c", &model);
        assert_eq!((cache.names(), cache.get("b").is_none()), (vec!["c".to_string(), "a".to_string()], true));
        cache.put("a", &model);
        assert_eq!(cache.names(), ["a", "c"]);
    }

    /// A predefined mixture by name (any case): on `Parity` at v8.0.0's mole fractions; on `Corrected` at those its
    /// ASHRAE 34 mass fractions give with the registry's molar masses (R410A, 50/50 by mass), and at v8.0.0's where the
    /// standard does not define it (R412A); a name v8.0.0 does not define is unknown.
    #[test]
    fn predefined_mixtures_take_their_data_set_s_fractions() {
        let composition = |set: DataSet, name: &str| {
            let fluid = predefined(&registry(set), name, set).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let mixture =
                (fluid.model() as &dyn std::any::Any).downcast_ref::<Mixture>().map(|m| m.composition().to_vec());
            mixture.unwrap_or_default()
        };
        let stored = |name: &str| PREDEFINED.iter().find(|m| m.name == name).map(|m| m.mole_fractions.to_vec());
        assert_eq!(Some(composition(DataSet::Parity, "r410a")), stored("R410A"));
        let corrected = registry(DataSet::Corrected);
        let m = ["R32", "R125"].map(|c| corrected.get(c).unwrap().info().molar_mass());
        let n = [0.5 / m[0], 0.5 / m[1]];
        assert_eq!(composition(DataSet::Corrected, "R410A"), vec![n[0] / (n[0] + n[1]), n[1] / (n[0] + n[1])]);
        assert_eq!(Some(composition(DataSet::Corrected, "Air")), stored("Air"));
        let unknown = Err(Error::Load(LoadError::UnknownName("R999.mix".into())));
        assert_eq!(predefined(&corrected, "R999", DataSet::Corrected).map(|f| f.info().molar_mass()), unknown);
    }

    /// A mixture name's components (PLAN.md M21.5) as it writes them, zeros kept, under either prefix; a predefined
    /// mixture's as the library lists them; none for a name that is no mixture name; a malformed name and an unknown
    /// predefined mixture refused as resolving them is.
    #[test]
    fn a_mixture_names_components() {
        assert_eq!(components("HEOS::R32[0.697615]&R125[0.302385]"), Some(Ok(vec!["R32", "R125"])));
        assert_eq!(components("GERG2008::Methane[0.9]&Ethane[0.1]"), Some(Ok(vec!["Methane", "Ethane"])));
        assert_eq!(components("GERG2008::Methane"), Some(Ok(vec!["Methane"])));
        assert_eq!(components("R32[1.0]&R125[0.0]"), Some(Ok(vec!["R32", "R125"])));
        assert_eq!(components("r410a.MIX"), Some(Ok(vec!["R32", "R125"])));
        assert_eq!(components("Air.mix"), Some(Ok(vec!["NITROGEN", "ARGON", "OXYGEN"])));
        for none in ["Water", "R410A", "HEOS::Water"] {
            assert_eq!(components(none), None, "{none}");
        }
        let names = with_mixtures(&registry(DataSet::Corrected), DataSet::Corrected);
        for bad in ["R32[0.5]&R125", "R999.mix"] {
            let refusal = names.resolve(bad).err();
            assert!(refusal.is_some(), "{bad}");
            assert_eq!(components(bad).and_then(Result::err), refusal, "{bad}");
        }
    }
}
