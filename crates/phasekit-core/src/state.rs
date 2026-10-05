//! L4 `State`: a small `Copy` value (≤ 256 B, asserted) holding everything first-order properties need.
//! No `Arc`, no cache, no setters (map 01 R11-R13, kernel-performance K1). Built by a flash, or by any
//! family through [`State::from_total`] (one phase) and [`State::from_split`] (two coexisting phases).

use crate::derivs::Bundle;
use crate::error::{DomainError, Error};
use crate::fluid::Gauge;
use crate::math;
use crate::model::ModelKey;
use crate::prop::{Partial, Prop};
use crate::relations::{self, At};
use crate::units::{Basis, Quality};

/// Phase label of a result. Never "unknown" or "not imposed" (map 01 U4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Phase {
    /// Subcritical liquid.
    Liquid,
    /// Subcritical gas.
    Gas,
    /// Two coexisting phases: vapour-liquid, or solid-vapour / solid-liquid for families that build them.
    TwoPhase,
    /// T > Tc and p > pc.
    Supercritical,
    /// T > Tc, p < pc.
    SupercriticalGas,
    /// T < Tc, p > pc.
    SupercriticalLiquid,
    /// At the critical point.
    CriticalPoint,
    /// A solid phase (Gibbs families, D10).
    Solid,
}

/// Which strategy produced a state: tests assert WHICH path ran (map 12 R3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Strategy {
    /// No iteration: (T, ρ) given.
    Direct,
    /// Built outside the core flash (a family's own flash).
    External,
    /// Safeguarded Newton on p(ρ) at fixed T.
    DensityNewton,
    /// Superancillary evaluation, accepted as exact.
    Superancillary,
    /// Vapour-liquid equilibrium solve (or polish of a `Guess` saturation curve).
    Vle,
}

/// The strategy and its iteration count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SolvePath {
    /// Strategy that produced the state.
    pub strategy: Strategy,
    /// Iterations it spent.
    pub iterations: u16,
}

/// One phase point. It carries its own T: the bubble and dew points of a pseudo-pure fluid differ (E3).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    t: f64,
    rho: f64,
    b: Bundle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Body {
    Single(Point),
    TwoPhase { q: f64, liquid: Point, vapour: Point },
}

/// A thermodynamic state. Fields are private so it can grow without breaking users.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    t: f64,
    p: f64,
    phase: Phase,
    body: Body,
    r: f64,
    molar_mass: f64,
    gauge: Gauge,
    path: SolvePath,
    key: ModelKey,
    extrapolated: bool,
}

const EXTERNAL: SolvePath = SolvePath { strategy: Strategy::External, iterations: 0 };

impl State {
    /// The states-of-matter seam (D10): any family, Helmholtz or Gibbs, builds a single-phase state from its
    /// order-2 total bundle at (T, ρ). Every property relation is then reused unchanged.
    pub fn from_total(
        key: ModelKey,
        t: f64,
        rho: f64,
        r: f64,
        molar_mass: f64,
        phase: Phase,
        total: &Bundle,
    ) -> Result<State, Error> {
        Self::single(key, t, rho, r, molar_mass, phase, total, EXTERNAL)
    }

    /// Two coexisting phases at molar fraction `q` of `vapour` (E2): a family's QT/PQ flash, IF97 across
    /// the dome, a mixture split, ice + vapour. `liquid` is the denser phase (a solid, for sublimation).
    /// Each phase keeps its own T and ρ; `t` and `p` are the state's reported values (equal to the phases'
    /// for a pure fluid; the pseudo-pure rule decides them otherwise, D4). Both phases must come from the
    /// same model. The split is extrapolated if either phase is.
    pub fn from_split(liquid: State, vapour: State, q: Quality, t: f64, p: f64) -> Result<State, Error> {
        Self::split(liquid, vapour, q, t, p, EXTERNAL)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn single(
        key: ModelKey,
        t: f64,
        rho: f64,
        r: f64,
        molar_mass: f64,
        phase: Phase,
        b: &Bundle,
        path: SolvePath,
    ) -> Result<State, Error> {
        let bad = |reason| Err(Error::InvalidState { reason });
        if !([t, rho, r, molar_mass].iter().all(|x| x.is_finite() && *x > 0.0)) {
            return bad("T, ρ, R and M must be finite and > 0");
        }
        if phase == Phase::TwoPhase {
            return bad("a single point cannot be labelled TwoPhase; use State::from_split");
        }
        if !(b.a00.is_finite() && b.a10.is_finite() && b.a01.is_finite()) {
            return bad("the first-order bundle (p, h, s, u) must be finite");
        }
        // E17: second-order entries may be infinite at a non-analytic critical point (cv diverges); then
        // only cv, cp, w and partial derivatives are `Undefined`, and p, h, s survive.
        if [b.a20, b.a11, b.a02].iter().any(|x| x.is_nan()) {
            return bad("NaN in the second-order bundle");
        }
        let p = relations::pressure(r, t, rho, b);
        let body = Body::Single(Point { t, rho, b: *b });
        Ok(State { t, p, phase, body, r, molar_mass, gauge: Gauge::NATIVE, path, key, extrapolated: false })
    }

    pub(crate) fn split(
        liquid: State,
        vapour: State,
        q: Quality,
        t: f64,
        p: f64,
        path: SolvePath,
    ) -> Result<State, Error> {
        let (Body::Single(l), Body::Single(v)) = (liquid.body, vapour.body) else {
            return Err(Error::InvalidState { reason: "a split needs two single-phase states" });
        };
        if liquid.key != vapour.key {
            return Err(Error::ForeignState);
        }
        if liquid.r != vapour.r || liquid.molar_mass != vapour.molar_mass {
            return Err(Error::InvalidState { reason: "the phases of a split must share R and M" });
        }
        if !(t.is_finite() && t > 0.0 && p.is_finite() && p > 0.0) {
            return Err(Error::InvalidState { reason: "T and p of a split must be finite and > 0" });
        }
        let body = Body::TwoPhase { q: q.get(), liquid: l, vapour: v };
        let extrapolated = liquid.extrapolated || vapour.extrapolated;
        Ok(State { t, p, phase: Phase::TwoPhase, body, path, extrapolated, ..liquid })
    }

    pub(crate) fn with_gauge(mut self, gauge: Gauge) -> State {
        self.gauge = gauge;
        self
    }

    /// Flags a state evaluated outside the model's validated domain: a metastable single-phase state under
    /// `DomainPolicy::Extrapolate` (D6). The core flash calls it; a family's own flash does the same.
    pub fn mark_extrapolated(self) -> State {
        State { extrapolated: true, ..self }
    }

    /// True when the state lies outside the model's validated domain (only under `DomainPolicy::Extrapolate`).
    pub fn is_extrapolated(&self) -> bool {
        self.extrapolated
    }

    /// Temperature, K.
    pub fn t(&self) -> f64 {
        self.t
    }
    /// Pressure, Pa.
    pub fn p(&self) -> f64 {
        self.p
    }
    /// Phase label.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// The strategy that produced this state.
    pub fn path(&self) -> SolvePath {
        self.path
    }
    /// Key of the model that produced this state.
    pub fn key(&self) -> ModelKey {
        self.key
    }
    /// The gas constant of the model that produced this state, J/(mol K).
    pub fn gas_constant(&self) -> f64 {
        self.r
    }
    /// Vapour quality; `None` for single-phase states (no −1 sentinel).
    pub fn quality(&self) -> Option<f64> {
        match self.body {
            Body::TwoPhase { q, .. } => Some(q),
            Body::Single(_) => None,
        }
    }

    fn per(&self, basis: Basis) -> f64 {
        match basis {
            Basis::Molar => 1.0,
            Basis::Mass => 1.0 / self.molar_mass,
        }
    }

    /// Quality-weighted molar value of `f` over the phase points.
    fn mix(&self, f: impl Fn(&Point) -> f64) -> f64 {
        match &self.body {
            Body::Single(pt) => f(pt),
            Body::TwoPhase { q, liquid, vapour } => (1.0 - q) * f(liquid) + q * f(vapour),
        }
    }

    /// Density in the given basis.
    pub fn rho(&self, basis: Basis) -> f64 {
        1.0 / (self.mix(|pt| 1.0 / pt.rho) * self.per(basis))
    }
    /// Enthalpy in the handle's reference state.
    pub fn h(&self, basis: Basis) -> f64 {
        (self.mix(|pt| relations::enthalpy(self.r, pt.t, &pt.b)) + self.gauge.dh()) * self.per(basis)
    }
    /// Entropy in the handle's reference state.
    pub fn s(&self, basis: Basis) -> f64 {
        (self.mix(|pt| relations::entropy(self.r, &pt.b)) + self.gauge.ds()) * self.per(basis)
    }
    /// Internal energy in the handle's reference state.
    pub fn u(&self, basis: Basis) -> f64 {
        (self.mix(|pt| relations::internal_energy(self.r, pt.t, &pt.b)) + self.gauge.dh()) * self.per(basis)
    }
    /// Compressibility factor Z = p/(ρRT).
    pub fn z(&self) -> f64 {
        self.p / (self.rho(Basis::Molar) * self.r * self.t)
    }

    /// The single phase point, or `Undefined` in the dome (map 02 §6, map 10 R18).
    fn single_point(&self, prop: Prop) -> Result<&Point, Error> {
        match &self.body {
            Body::Single(pt) => Ok(pt),
            Body::TwoPhase { .. } => Err(Error::Undefined { prop, phase: self.phase }),
        }
    }

    /// `v` if finite, else `Undefined` (cv at a non-analytic critical point, cp where (∂p/∂ρ)_T = 0; E17).
    fn defined(&self, prop: Prop, v: f64) -> Result<f64, Error> {
        if v.is_finite() { Ok(v) } else { Err(Error::Undefined { prop, phase: self.phase }) }
    }

    /// Isochoric heat capacity; undefined in the dome and where it diverges.
    pub fn cv(&self, basis: Basis) -> Result<f64, Error> {
        let pt = self.single_point(Prop::Cvmolar)?;
        self.defined(Prop::Cvmolar, relations::cv(self.r, &pt.b) * self.per(basis))
    }
    /// Isobaric heat capacity; undefined in the dome and where it diverges.
    pub fn cp(&self, basis: Basis) -> Result<f64, Error> {
        let pt = self.single_point(Prop::Cpmolar)?;
        self.defined(Prop::Cpmolar, relations::cp(self.r, &pt.b) * self.per(basis))
    }
    /// Speed of sound, m/s; undefined in the dome and where it diverges.
    pub fn speed_of_sound(&self) -> Result<f64, Error> {
        let pt = self.single_point(Prop::SpeedOfSound)?;
        let w2 =
            self.defined(Prop::SpeedOfSound, relations::speed_of_sound_squared(self.r, pt.t, self.molar_mass, &pt.b))?;
        if w2 > 0.0 { Ok(w2.sqrt()) } else { Err(DomainError::MechanicallyUnstable.into()) }
    }
    /// A first partial derivative from the stored order-2 bundle: every family, no model call (E1).
    pub fn partial(&self, p: Partial) -> Result<f64, Error> {
        let prop = Prop::Partial(p);
        let pt = self.single_point(prop)?;
        let at = At { r: self.r, t: pt.t, rho: pt.rho, molar_mass: self.molar_mass, ds: self.gauge.ds(), b: &pt.b };
        relations::first_partial(p.of, p.wrt, p.at, &at).ok_or(Error::Undefined { prop, phase: self.phase })
    }

    /// (T, ρ) of the single phase point (what `ThermoModel::derivs` evaluates at); `None` in the dome.
    pub(crate) fn single_t_rho(&self) -> Option<(f64, f64)> {
        match self.body {
            Body::Single(pt) => Some((pt.t, pt.rho)),
            Body::TwoPhase { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: ModelKey = ModelKey::from_content(b"test");
    const R: f64 = 8.314_462_618;

    /// E17: at a non-analytic critical point A20 (cv) diverges. p, h and s survive; cv, cp and w are
    /// `Undefined` instead of the whole state failing.
    #[test]
    fn divergent_cv_keeps_first_order_properties() {
        let b = Bundle { a00: -0.1, a10: 1.2, a01: 0.3, a20: f64::NEG_INFINITY, a11: 0.5, a02: -0.6 };
        let s = State::from_total(KEY, 647.096, 17_873.7, R, 0.018_015_268, Phase::CriticalPoint, &b).unwrap();
        assert!(s.p().is_finite() && s.h(Basis::Mass).is_finite() && s.s(Basis::Mass).is_finite());
        let undefined = |prop| Err(Error::Undefined { prop, phase: Phase::CriticalPoint });
        assert_eq!(s.cv(Basis::Molar), undefined(Prop::Cvmolar));
        assert_eq!(s.cp(Basis::Molar), undefined(Prop::Cpmolar));
        assert_eq!(s.speed_of_sound(), undefined(Prop::SpeedOfSound));
        let nan = Bundle { a20: f64::NAN, ..b };
        assert!(matches!(
            State::from_total(KEY, 300.0, 1.0, R, 0.018, Phase::Gas, &nan),
            Err(Error::InvalidState { .. })
        ));
        let bad_p = Bundle { a01: f64::INFINITY, ..b };
        assert!(matches!(
            State::from_total(KEY, 300.0, 1.0, R, 0.018, Phase::Gas, &bad_p),
            Err(Error::InvalidState { .. })
        ));
    }

    #[test]
    fn split_validates_its_phases() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let gas = State::from_total(KEY, 300.0, 40.0, R, 0.04, Phase::Gas, &b).unwrap();
        let liquid = State::from_total(KEY, 300.0, 4000.0, R, 0.04, Phase::Liquid, &b).unwrap();
        let q = Quality::new(0.25).unwrap();
        let s = State::from_split(liquid, gas, q, 300.0, gas.p()).unwrap();
        assert_eq!((s.phase(), s.quality()), (Phase::TwoPhase, Some(0.25)));
        assert_eq!(
            State::from_split(s, gas, q, 300.0, 1e5),
            Err(Error::InvalidState { reason: "a split needs two single-phase states" })
        );
        let other = State::from_total(ModelKey::from_content(b"other"), 300.0, 40.0, R, 0.04, Phase::Gas, &b).unwrap();
        assert_eq!(State::from_split(liquid, other, q, 300.0, 1e5), Err(Error::ForeignState));
    }
}
