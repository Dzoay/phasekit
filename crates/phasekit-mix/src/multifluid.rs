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
use crate::data::{DepartureData, PairData};
use crate::departure;
use crate::generated::{CAS, DEPARTURES, PAIRS};
use crate::reducing::ReducingFunctions;

/// One component: its fluid and the state its residual reduces by.
#[derive(Clone, Debug)]
struct Component {
    fluid: Fluid,
    t_r: f64,
    rho_r: f64,
}

/// A pair with a departure function: its components, F and the function.
#[derive(Clone, Copy, Debug)]
struct Departure {
    i: usize,
    j: usize,
    f: f64,
    function: &'static DepartureData,
}

/// The multi-fluid model of a set of components, for any composition of them: their fluids, the reducing functions
/// of every pair and the departure functions of those with F ≠ 0. Immutable; share it through an `Arc`.
#[derive(Clone, Debug)]
pub struct MultiFluid {
    components: Arc<[Component]>,
    reducing: ReducingFunctions,
    departures: Arc<[Departure]>,
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
fn cas(name: &str) -> Option<&'static str> {
    CAS.binary_search_by(|(n, _)| (*n).cmp(name)).ok().and_then(|i| CAS.get(i)).map(|(_, cas)| *cas)
}

impl MultiFluid {
    /// The model of `names`, each resolved in `registry` (name, alias or CAS), with the library's pair parameters as
    /// `data_set` gives them (`Corrected`: [`DataSet`]'s corrections, DIV-0013; `Parity`: v8.0.0's). Refused: fewer than
    /// two components, a name twice, a component without a Helmholtz model or a reducing state, one outside v8.0.0's
    /// library, and a pair the library does not have (`UnknownName("binary pair A&B")`; v8.0.0 refuses it too,
    /// ROT-292).
    pub fn new(registry: &Registry, names: &[&str], data_set: DataSet) -> Result<MultiFluid, Error> {
        let components = names.iter().map(|name| component(registry, name)).collect::<Result<Vec<_>, _>>()?;
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
        let states: Vec<(f64, f64)> = components.iter().map(|c| (c.t_r, c.rho_r)).collect();
        let reducing = ReducingFunctions::new(&states, &parameters);
        Ok(MultiFluid { components: components.into(), reducing, departures: departures.into() })
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
            let (w, jet) = (x[d.i] * x[d.j] * d.f, departure::jet(d.function, tau, 0.0));
            let at = |i, j| jet.derivative(i, j).unwrap_or(f64::NAN);
            (b, c) = (b + w * at(0, 1), c + w * at(0, 2));
            (db, dc) = (db + w * at(1, 1) * dtau_dt, dc + w * at(1, 2) * dtau_dt);
        }
        let (r1, r2) = (1.0 / rho_r, 1.0 / (rho_r * rho_r));
        Some(Virials { b: b * r1, c: c * r2, db_dt: db * r1, dc_dt: dc * r2 })
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

/// The component `name` resolves to, with the state its Helmholtz model reduces by.
fn component(registry: &Registry, name: &str) -> Result<Component, Error> {
    let fluid = registry.resolve(name)?;
    let reducing = fluid.model().helmholtz().and_then(|model| model.reducing()).ok_or_else(|| unknown(name))?;
    Ok(Component { fluid, t_r: reducing.t, rho_r: reducing.rho })
}
