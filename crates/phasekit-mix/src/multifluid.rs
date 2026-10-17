//! The multi-fluid residual (Kunz and Wagner 2012; the model of CoolProp's HEOS mixtures; PLAN.md M13.3):
//!
//! α^r(τ, δ, x) = Σ_i x_i α^r_oi(τ, δ) + Σ_{i<j} x_i x_j F_ij α^r_ij(τ, δ), τ = T_r(x)/T, δ = ρ/ρ_r(x),
//!
//! each component's α^r_oi its own model's `residual` at (T_r,i/τ, δ ρ_r,i), the state that gives it the mixture's τ
//! and δ (`HelmholtzModel::reducing`), and α^r_ij a pair's departure function, evaluated only where F ≠ 0 (ROT-195).

use std::cmp::Ordering;
use std::sync::Arc;

use phasekit_core::{DataSet, Derivs, Error, Fluid, LoadError, Order, Registry, Virials};

use crate::corrections;
use crate::data::{DepartureData, PairData, Reducing};
use crate::departure;
use crate::generated::{CAS, DEPARTURES, PAIRS};
use crate::gerg;
use crate::mixture::R_CODATA;
use crate::reducing::ReducingFunctions;

/// One component: its fluid and the state its residual reduces by.
#[derive(Clone, Debug)]
pub(crate) struct Component {
    pub(crate) fluid: Fluid,
    pub(crate) t_r: f64,
    pub(crate) rho_r: f64,
}

/// A pair with a departure function: its components, F and the function.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Departure {
    pub(crate) i: usize,
    pub(crate) j: usize,
    pub(crate) f: f64,
    pub(crate) function: &'static DepartureData,
}

/// Which multi-fluid model: CoolProp 8.0.0's library over the registry's fluids, or GERG-2008 over its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Library,
    Gerg2008,
}

/// The multi-fluid model of a set of components, for any composition of them: their fluids, the reducing functions
/// of every pair and the departure functions of those with F ≠ 0. Immutable; share it through an `Arc`.
#[derive(Clone, Debug)]
pub struct MultiFluid {
    pub(crate) components: Arc<[Component]>,
    pub(crate) reducing: ReducingFunctions,
    pub(crate) departures: Arc<[Departure]>,
    family: Family,
}

/// A binary pair's parameters in a model, from its first component to its second (Kunz and Wagner 2012; GERG TM15
/// §7.2): the reducing functions' β_T, γ_T, β_v and γ_v, Lemmon's ξ and ζ as the β = 1 and γ they convert to, and F,
/// the weight of the pair's departure function (0 where it has none). Read with [`MultiFluid::binary`], overridden with
/// [`MultiFluid::with_binary`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Binary {
    /// β_T.
    pub beta_t: f64,
    /// γ_T.
    pub gamma_t: f64,
    /// β_v.
    pub beta_v: f64,
    /// γ_v.
    pub gamma_v: f64,
    /// F.
    pub f: f64,
}

/// v8.0.0's pair of two CAS numbers, and whether `(a, b)` is the reverse of its order; none for a number with itself.
fn pair(a: &str, b: &str) -> Option<(&'static PairData, bool)> {
    let (key, swapped) = match a.cmp(b) {
        Ordering::Less => ([a, b], false),
        Ordering::Greater => ([b, a], true),
        Ordering::Equal => return None,
    };
    PAIRS
        .binary_search_by(|p| p.cas.as_slice().cmp(key.as_slice()))
        .ok()
        .and_then(|i| PAIRS.get(i))
        .map(|p| (p, swapped))
}

/// The CAS number of a library fluid by its canonical name.
pub(crate) fn cas(name: &str) -> Option<&'static str> {
    CAS.binary_search_by(|(n, _)| (*n).cmp(name)).ok().and_then(|i| CAS.get(i)).map(|(_, cas)| *cas)
}

impl MultiFluid {
    /// The model of `names`, each resolved in `registry` (name, alias or CAS), with the library's pair parameters as
    /// `data_set` gives them (`Corrected`: [`DataSet`]'s corrections, DIV-0013; `Parity`: v8.0.0's). Refused: fewer than
    /// two components, a name twice, a component without a Helmholtz model or a reducing state, one outside v8.0.0's
    /// library, and a pair the library does not have (`UnknownName("binary pair A&B")`; v8.0.0 refuses it too,
    /// ROT-292).
    pub fn new(registry: &Registry, names: &[&str], data_set: DataSet) -> Result<MultiFluid, Error> {
        let components =
            names.iter().map(|name| component(&registry.resolve(name)?, name)).collect::<Result<Vec<_>, _>>()?;
        let keys = components
            .iter()
            .map(|c| cas(c.fluid.info().name()).ok_or_else(|| unknown(c.fluid.info().name())))
            .collect::<Result<Vec<_>, _>>()?;
        if components.len() < 2 || keys.iter().enumerate().any(|(i, k)| keys[..i].contains(k)) {
            return Err(Error::InvalidInput {
                quantity: "number of distinct components",
                value: components.len() as f64,
            });
        }
        let (mut parameters, mut departures) = (Vec::new(), Vec::new());
        for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                let (pair_data, swapped) = pair(keys[i], keys[j]).ok_or_else(|| {
                    unknown(&format!(
                        "binary pair {}&{}",
                        components[i].fluid.info().name(),
                        components[j].fluid.info().name()
                    ))
                })?;
                let reducing = match data_set {
                    DataSet::Parity => pair_data.reducing,
                    _ => corrections::corrected(pair_data.cas, pair_data.reducing),
                };
                parameters.push((i, j, reducing, swapped));
                if let Some(function) = pair_data.departure.and_then(|k| DEPARTURES.get(k)) {
                    departures.push(Departure { i, j, f: pair_data.f, function });
                }
            }
        }
        Ok(MultiFluid::from_parts(components, &parameters, departures, Family::Library))
    }

    /// GERG-2008's model of `names` (PLAN.md M13.5), each resolved in `registry` (name, alias or CAS) to one of its 21
    /// components, which take GERG's own equations; its 210 binaries all have parameters, and 15 a departure function.
    /// Refused: fewer than two components, a component twice, and one outside GERG-2008
    /// (`UnknownName("GERG2008::<name>")`).
    pub fn gerg2008(registry: &Registry, names: &[&str]) -> Result<MultiFluid, Error> {
        let resolved = names.iter().map(|name| gerg::component(registry, name)).collect::<Result<Vec<_>, _>>()?;
        let index: Vec<usize> = resolved.iter().map(|(i, _)| *i).collect();
        if index.len() < 2 || index.iter().enumerate().any(|(k, i)| index[..k].contains(i)) {
            return Err(Error::InvalidInput { quantity: "number of distinct components", value: index.len() as f64 });
        }
        let mut components = Vec::with_capacity(resolved.len());
        for (i, fluid) in &resolved {
            let fluid = Fluid::new(Arc::new(gerg::pure_fluid(*i, fluid)?));
            components.push(component(&fluid, fluid.info().name())?);
        }
        let (mut parameters, mut departures) = (Vec::new(), Vec::new());
        for a in 0..index.len() {
            for b in a + 1..index.len() {
                let (binary, swapped) = gerg::binary(index[a], index[b]).ok_or_else(|| unknown("GERG-2008 binary"))?;
                let reducing = Reducing::Gerg([binary.beta_t, binary.gamma_t, binary.beta_v, binary.gamma_v]);
                parameters.push((a, b, reducing, swapped));
                if let Some(function) = binary.model.and_then(gerg::departure).filter(|_| binary.f != 0.0) {
                    departures.push(Departure { i: a, j: b, f: binary.f, function });
                }
            }
        }
        Ok(MultiFluid::from_parts(components, &parameters, departures, Family::Gerg2008))
    }

    fn from_parts(
        components: Vec<Component>,
        parameters: &[(usize, usize, Reducing, bool)],
        departures: Vec<Departure>,
        family: Family,
    ) -> MultiFluid {
        let states: Vec<(f64, f64)> = components.iter().map(|c| (c.t_r, c.rho_r)).collect();
        let reducing = ReducingFunctions::new(&states, parameters);
        MultiFluid { components: components.into(), reducing, departures: departures.into(), family }
    }

    /// The model's molar gas constant, J/(mol·K): `R_CODATA` for the library's mixtures, `R_GERG` for GERG-2008's (user
    /// decision MX3).
    pub fn gas_constant(&self) -> f64 {
        match self.family {
            Family::Library => R_CODATA,
            Family::Gerg2008 => gerg::R_GERG,
        }
    }

    /// The prefix of the model's mixture names: none for the library's, `GERG2008::` for GERG-2008's (MX6).
    pub(crate) fn prefix(&self) -> &'static str {
        match self.family {
            Family::Library => "",
            Family::Gerg2008 => gerg::PREFIX,
        }
    }

    /// The number of components.
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Never: a multi-fluid model has at least two components.
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// The number of pairs that carry a departure function (F ≠ 0), the only ones evaluated (ROT-195).
    pub fn departures(&self) -> usize {
        self.departures.len()
    }

    /// The reducing state (T_r, ρ_r) at mole fractions `x` (K, mol/m³).
    pub fn reducing_state(&self, x: &[f64]) -> (f64, f64) {
        (self.reducing.t_r(x), self.reducing.rho_r(x))
    }

    /// The residual's scaled derivatives A_ij = τ^i δ^j ∂^(i+j)α^r/∂τ^i∂δ^j at (T, ρ) and mole fractions `x`, to
    /// `order`, at fixed composition. `x` has one fraction per component.
    pub fn residual(&self, x: &[f64], t: f64, rho: f64, order: Order) -> Result<Derivs, Error> {
        if x.len() != self.components.len() {
            return Err(Error::InvalidInput { quantity: "number of mole fractions", value: x.len() as f64 });
        }
        Ok(self.residual_at(x, t, rho, order))
    }

    /// [`residual`](Self::residual) for an `x` of the components' number, which the caller guarantees.
    pub(crate) fn residual_at(&self, x: &[f64], t: f64, rho: f64, order: Order) -> Derivs {
        let (t_r, rho_r) = self.reducing_state(x);
        let (tau, delta) = (t_r / t, rho / rho_r);
        let mut parts = Vec::with_capacity(self.components.len() + self.departures.len());
        for (c, xi) in self.components.iter().zip(x) {
            // `component` admitted only fluids with a Helmholtz model.
            let part = c.fluid.model().helmholtz().map(|model| model.residual(c.t_r / tau, delta * c.rho_r, order));
            parts.push((*xi, part.unwrap_or_else(|| Derivs::from_fn(order, |_, _| f64::NAN))));
        }
        for d in self.departures.iter() {
            parts.push((x[d.i] * x[d.j] * d.f, departure::derivs(d.function, tau, delta, order)));
        }
        Derivs::from_fn(order, |i, j| parts.iter().map(|(w, part)| w * part.get(i, j).unwrap_or(f64::NAN)).sum())
    }

    /// The exact virial coefficients at T and mole fractions `x` (E4): B = α^r_δ(τ, 0)/ρ_r and C = α^r_δδ(τ, 0)/ρ_r²,
    /// each component's from its own `zero_density` at T_r,i T/T_r (where its τ is the mixture's), each departure's from
    /// its jet at δ = 0; `None` when a component has no closed form.
    pub(crate) fn virials(&self, x: &[f64], t: f64) -> Option<Virials> {
        let (t_r, rho_r) = self.reducing_state(x);
        let tau = t_r / t;
        let (mut b, mut c, mut db, mut dc) = (0.0, 0.0, 0.0, 0.0);
        for (component, xi) in self.components.iter().zip(x) {
            let v = component.fluid.model().helmholtz()?.zero_density(component.t_r / tau)?;
            let (rho, scale) = (component.rho_r, component.t_r / t_r);
            (b, c) = (b + xi * rho * v.b, c + xi * rho * rho * v.c);
            (db, dc) = (db + xi * rho * v.db_dt * scale, dc + xi * rho * rho * v.dc_dt * scale);
        }
        let dtau_dt = -t_r / (t * t);
        for d in self.departures.iter() {
            let (w, jet) = (x[d.i] * x[d.j] * d.f, departure::jet(d.function, tau, 0.0, 3));
            let at = |i, j| jet.derivative(i, j).unwrap_or(f64::NAN);
            (b, c) = (b + w * at(0, 1), c + w * at(0, 2));
            (db, dc) = (db + w * at(1, 1) * dtau_dt, dc + w * at(1, 2) * dtau_dt);
        }
        let (r1, r2) = (1.0 / rho_r, 1.0 / (rho_r * rho_r));
        Some(Virials { b: b * r1, c: c * r2, db_dt: db * r1, dc_dt: dc * r2 })
    }

    /// Pair (i, j)'s parameters, from component i to j: those of (j, i) with β inverted. None for i = j or a component
    /// outside the model.
    pub fn binary(&self, i: usize, j: usize) -> Option<Binary> {
        let [beta_t, gamma_t, beta_v, gamma_v] = self.reducing.parameters(i, j)?;
        let (a, b) = (i.min(j), i.max(j));
        let f = self.departures.iter().find(|d| (d.i, d.j) == (a, b)).map_or(0.0, |d| d.f);
        Some(Binary { beta_t, gamma_t, beta_v, gamma_v, f })
    }

    /// A new model with pair (i, j)'s parameters, from component i to j, replaced by `binary` (ROT-196): this model and
    /// the library stay as they were, and so does every model built from them. F = 0 leaves the pair's departure
    /// function out (ROT-195). Refused (`InvalidInput`): i = j or a component outside the model, a β or γ not finite and
    /// positive, an F not finite, and an F ≠ 0 for a pair without a departure function.
    pub fn with_binary(&self, i: usize, j: usize, binary: Binary) -> Result<MultiFluid, Error> {
        let Binary { beta_t, gamma_t, beta_v, gamma_v, f } = binary;
        let parameters = [beta_t, gamma_t, beta_v, gamma_v];
        if let Some(bad) = parameters.into_iter().find(|p| !(p.is_finite() && *p > 0.0)) {
            return Err(Error::InvalidInput { quantity: "binary parameter", value: bad });
        }
        let pair = Error::InvalidInput { quantity: "binary pair's component", value: i.max(j) as f64 };
        let reducing = self.reducing.with_parameters(i, j, parameters).ok_or(pair)?;
        let (a, b) = (i.min(j), i.max(j));
        let has_departure = self.departures.iter().any(|d| (d.i, d.j) == (a, b));
        if !f.is_finite() || (f != 0.0 && !has_departure) {
            return Err(Error::InvalidInput { quantity: "departure function's F", value: f });
        }
        let departures = self
            .departures
            .iter()
            .filter_map(|d| if (d.i, d.j) == (a, b) { (f != 0.0).then_some(Departure { f, ..*d }) } else { Some(*d) })
            .collect();
        Ok(MultiFluid { reducing, departures, ..self.clone() })
    }

    /// The components' fluids, in order.
    pub fn fluids(&self) -> impl Iterator<Item = &Fluid> {
        self.components.iter().map(|c| &c.fluid)
    }
}

/// A name nothing resolves, as the registry reports one.
fn unknown(name: &str) -> Error {
    Error::Load(LoadError::UnknownName(name.into()))
}

/// `fluid` as a component, with the state its Helmholtz model reduces by; `name` is what a refusal names.
fn component(fluid: &Fluid, name: &str) -> Result<Component, Error> {
    let reducing = fluid.model().helmholtz().and_then(|model| model.reducing()).ok_or_else(|| unknown(name))?;
    Ok(Component { fluid: fluid.clone(), t_r: reducing.t, rho_r: reducing.rho })
}

#[cfg(test)]
mod tests {
    use phasekit_core::{DataSet, Error, LoadError, Order, Registry};

    use super::*;

    fn registry() -> Registry {
        Registry::from_embedded(DataSet::Corrected).unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// Rot: ROT-292 (map 12 COO-26). v8.0.0's library has no Ammonia/Water pair, so its `factory("HEOS",
    /// "Ammonia&Water")` fails at construction (open since #341, 2014). Here the model is refused typed, naming the pair,
    /// whether built from the library or by a mixture name, and never estimated: a third component does not hide it.
    #[test]
    fn ammonia_water_is_no_model() {
        let registry = registry();
        let missing = |pair: &str| Some(Error::Load(LoadError::UnknownName(format!("binary pair {pair}").into())));
        let model = |names: &[&str]| MultiFluid::new(&registry, names, DataSet::Corrected).err();
        assert_eq!(model(&["Ammonia", "Water"]), missing("Ammonia&Water"));
        assert_eq!(model(&["Water", "Ammonia"]), missing("Water&Ammonia"));
        assert_eq!(model(&["Ammonia", "Water", "Methane"]), missing("Ammonia&Water"));
        let named = crate::with_mixtures(&registry, DataSet::Corrected).resolve("Ammonia[0.4]&Water[0.6]");
        assert_eq!(named.err(), missing("Ammonia&Water"));
    }

    /// Rot: ROT-196 (map 04 #5). v8.0.0's binary-interaction setters write its process-wide library, so an override,
    /// or a load that fails part-way, reaches every mixture built after it. Here an override builds a new model and
    /// changes nothing else: R32/R125's β_T raised 1 % moves the new model's T_r and α^r, while the model it came from
    /// and one built afresh after it keep the library's values bit for bit. The pair reads back from either end (β
    /// inverted, γ kept), and is written from either end; its library F is 1, F = 0.5 halves its departure function's
    /// share of α^r, and F = 0 drops the function. Refused: a pair outside the model or of one component, a β or γ not
    /// finite and positive, an F not finite, and an F on a pair that has no departure function.
    #[test]
    fn bip_override_never_poisons_the_defaults() {
        let registry = registry();
        let build = |names: &[&str]| MultiFluid::new(&registry, names, DataSet::Corrected).unwrap();
        let defaults = build(&["R32", "R125"]);
        let x = [0.4, 0.6];
        let at = |m: &MultiFluid| (m.reducing_state(&x), m.residual(&x, 300.0, 5000.0, Order::Two).unwrap());
        let before = at(&defaults);
        let b = defaults.binary(0, 1).unwrap();
        let raised = Binary { beta_t: 1.01 * b.beta_t, ..b };
        let changed = defaults.with_binary(0, 1, raised).unwrap();
        assert!(at(&changed).0.0 != before.0.0 && at(&changed).1 != before.1, "{:?}", at(&changed));
        assert_eq!((at(&defaults), at(&build(&["R32", "R125"]))), (before, before));
        assert_eq!(changed.binary(0, 1), Some(raised));
        let back = changed.binary(1, 0).unwrap();
        let inverted = Binary { beta_t: 1.0 / raised.beta_t, beta_v: 1.0 / raised.beta_v, ..raised };
        assert_eq!(back, inverted);
        // β of 2 and 0.5 invert exactly, so a pair written from its far end reads back bit for bit from either.
        let far = Binary { beta_t: 2.0, beta_v: 0.5, ..b };
        let written = defaults.with_binary(1, 0, far).unwrap();
        assert_eq!(
            (written.binary(1, 0), written.binary(0, 1)),
            (Some(far), Some(Binary { beta_t: 0.5, beta_v: 2.0, ..far }))
        );
        let departure = |f: f64| {
            let m = defaults.with_binary(0, 1, Binary { f, ..b }).unwrap();
            (m.departures(), m.residual(&x, 300.0, 5000.0, Order::One).unwrap().get(0, 0).unwrap_or(f64::NAN))
        };
        let ((one, full), (_, half), (none, without)) = (departure(1.0), departure(0.5), departure(0.0));
        assert_eq!((b.f, one, none), (1.0, 1, 0));
        assert!(((half - without) / (full - without) - 0.5).abs() < 1e-12, "{full} {half} {without}");
        let refused = |i, j, binary| defaults.with_binary(i, j, binary).is_err();
        assert!(refused(0, 2, b) && refused(1, 1, b));
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let each = [
                Binary { beta_t: bad, ..b },
                Binary { gamma_t: bad, ..b },
                Binary { beta_v: bad, ..b },
                Binary { gamma_v: bad, ..b },
            ];
            assert!(each.into_iter().all(|binary| refused(0, 1, binary)), "{bad}");
        }
        assert!(refused(0, 1, Binary { f: f64::NAN, ..b }));
        let plain = build(&["CarbonDioxide", "Ethane"]);
        assert_eq!(plain.binary(0, 1).map(|p| p.f), Some(0.0));
        assert!(plain.with_binary(0, 1, Binary { f: 1.0, ..plain.binary(0, 1).unwrap() }).is_err());
    }
}
