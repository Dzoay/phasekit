//! L4 `State`: a small `Copy` value (≤ 256 B, asserted) holding everything first-order properties need.
//! No `Arc`, no cache, no setters (map 01 R11-R13, kernel-performance K1). Built by a flash, or by any
//! family through [`State::from_total`] (one phase) and [`State::from_split`] (two coexisting phases).

use crate::derivs::{Bundle, Derivs};
use crate::error::{DomainError, Error};
use crate::fluid::Gauge;
use crate::math;
use crate::model::{ModelKey, Reducing};
use crate::prop::{DerivVar, Partial, Prop, SecondPartial};
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
    /// A pseudo-pure fluid's ancillary definition of saturation, each side's density the EOS's at its (T, p) (D4).
    Ancillary,
    /// TOMS 748 in T along an isobar (P+X), each iterate a density solve at (T, p).
    Isobar,
    /// Newton in (T, ln ρ) on a P+X pair's (p, x), one bundle an iterate, from the span's saturated end or Peng and
    /// Robinson's (T, ρ); [`Strategy::Isobar`] where it does not converge.
    TRhoNewton,
    /// TOMS 748 in T along an isochore (D+X), each iterate DT's state at (T, ρ).
    Isochore,
    /// TOMS 748 in ln ρ along an isotherm (T+X), on each monotonic piece of x(ρ), each iterate DT's state at (T, ρ).
    Isotherm,
    /// TOMS 748 in T along the saturation curve (Q-pairs), on the caloric curves and polished on the EOS.
    Saturation,
    /// TOMS 748 in ln p along an isentrope (HS, SU), each iterate a PS flash.
    Isentrope,
    /// TOMS 748 in T along a melting-curve segment, for its T_melt(p) (M8.11).
    Melting,
    /// Newton in (T₀, ρ₀) for an ECS conformal state: the reference fluid's α^r and Z equal the target's (map 05 U8).
    Conformal,
}

/// The strategy and its iteration count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SolvePath {
    /// Strategy that produced the state.
    pub strategy: Strategy,
    /// Iterations it spent.
    pub iterations: u16,
}

/// One phase point. It carries its own T, since the bubble and dew points of a pseudo-pure fluid differ (E3), and its own
/// molar mass, since a mixture's phases differ in composition (PLAN.md M13.1). Its bundle is order 2, except a flash's
/// two-phase state strictly inside the dome, whose phases carry order 1 (their second-order entries zero): there
/// [`State::saturated_point`] and [`State::one_phase`] refuse, so nothing reads them (user decision PF1).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    t: f64,
    rho: f64,
    m: f64,
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
    /// Each phase keeps its own T, ρ and molar mass; `t` and `p` are the state's reported values (equal to the phases'
    /// for a pure fluid; the pseudo-pure rule decides them otherwise, D4). Both phases must come from the
    /// same model and share its R; a mixture's phases differ in molar mass, and the split's mass-basis values and mass
    /// quality weigh them by it (PLAN.md M13.1). The split is extrapolated if either phase is.
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
        let body = Body::Single(Point { t, rho, m: molar_mass, b: *b });
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
        if liquid.r != vapour.r {
            return Err(Error::InvalidState { reason: "the phases of a split must share R" });
        }
        if !(t.is_finite() && t > 0.0 && p.is_finite() && p > 0.0) {
            return Err(Error::InvalidState { reason: "T and p of a split must be finite and > 0" });
        }
        let q = q.get();
        // The mass per mole of the whole. Phases of one molar mass (a pure fluid's) keep it, bit for bit.
        let molar_mass = if l.m == v.m { l.m } else { (1.0 - q) * l.m + q * v.m };
        let body = Body::TwoPhase { q, liquid: l, vapour: v };
        let extrapolated = liquid.extrapolated || vapour.extrapolated;
        Ok(State { t, p, phase: Phase::TwoPhase, body, molar_mass, path, extrapolated, ..liquid })
    }

    /// The same state with the path of the solve that found it.
    pub(crate) fn with_path(mut self, path: SolvePath) -> State {
        self.path = path;
        self
    }

    /// The reference-state offsets this state reports in.
    pub(crate) fn gauge(&self) -> Gauge {
        self.gauge
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

    /// Vapour quality on a mass basis: the vapour's share of the mass, qM″/((1 − q)M′ + qM″), the molar quality where
    /// the phases share one molar mass (a pure fluid's); `None` for single-phase states.
    pub fn mass_quality(&self) -> Option<f64> {
        match self.body {
            Body::TwoPhase { q, liquid, vapour } if liquid.m != vapour.m => Some(q * vapour.m / self.molar_mass),
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

    /// Density in the given basis: a single point's own, times M on a mass basis (CoolProp's `rhomass`), not the
    /// reciprocal of its reciprocal, which is an ulp off (M8.9: methanol's viscosity amplifies that past
    /// `TransportDirect`); in the dome the lever rule on molar volume.
    pub fn rho(&self, basis: Basis) -> f64 {
        match (&self.body, basis) {
            (Body::Single(pt), Basis::Molar) => pt.rho,
            (Body::Single(pt), Basis::Mass) => pt.rho * self.molar_mass,
            (Body::TwoPhase { .. }, _) => 1.0 / (self.mix(|pt| 1.0 / pt.rho) * self.per(basis)),
        }
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
    /// Gibbs energy in the handle's reference state (shifted by Δh − TΔs); for a pure fluid also its chemical
    /// potential.
    pub fn g(&self, basis: Basis) -> f64 {
        let (dh, ds) = (self.gauge.dh(), self.gauge.ds());
        (self.mix(|pt| relations::gibbs_energy(self.r, pt.t, &pt.b) - pt.t * ds) + dh) * self.per(basis)
    }
    /// Helmholtz energy in the handle's reference state (shifted by Δh − TΔs).
    pub fn a(&self, basis: Basis) -> f64 {
        let (dh, ds) = (self.gauge.dh(), self.gauge.ds());
        (self.mix(|pt| relations::helmholtz_energy(self.r, pt.t, &pt.b) - pt.t * ds) + dh) * self.per(basis)
    }
    /// Compressibility factor Z = p/(ρRT).
    pub fn z(&self) -> f64 {
        self.p / (self.rho(Basis::Molar) * self.r * self.t)
    }

    /// The point c_p, c_v, w and the partials read (SQ1, SQ2, as CoolProp): a single phase's own, a split's saturated
    /// liquid at Q = 0 and its vapour at Q = 1 (as `one_phase`), and `Undefined` strictly inside the dome (map 02 §6,
    /// map 10 R18).
    fn saturated_point(&self, prop: Prop) -> Result<&Point, Error> {
        match &self.body {
            Body::Single(pt)
            | Body::TwoPhase { q: 0.0, liquid: pt, .. }
            | Body::TwoPhase { q: 1.0, vapour: pt, .. } => Ok(pt),
            Body::TwoPhase { .. } => Err(Error::Undefined { prop, phase: self.phase }),
        }
    }

    /// `v` if finite, else `Undefined` (cv at a non-analytic critical point, cp where (∂p/∂ρ)_T = 0; E17).
    fn defined(&self, prop: Prop, v: f64) -> Result<f64, Error> {
        if v.is_finite() { Ok(v) } else { Err(Error::Undefined { prop, phase: self.phase }) }
    }

    /// Isochoric heat capacity; a saturated phase's at Q = 0 and 1, undefined strictly inside the dome and where it diverges.
    pub fn cv(&self, basis: Basis) -> Result<f64, Error> {
        let pt = self.saturated_point(Prop::Cvmolar)?;
        self.defined(Prop::Cvmolar, relations::cv(self.r, &pt.b) * self.per(basis))
    }
    /// Isobaric heat capacity; a saturated phase's at Q = 0 and 1, undefined strictly inside the dome and where it diverges.
    pub fn cp(&self, basis: Basis) -> Result<f64, Error> {
        let pt = self.saturated_point(Prop::Cpmolar)?;
        self.defined(Prop::Cpmolar, relations::cp(self.r, &pt.b) * self.per(basis))
    }
    /// Speed of sound, m/s; a saturated phase's at Q = 0 and 1, undefined strictly inside the dome and where it diverges.
    pub fn speed_of_sound(&self) -> Result<f64, Error> {
        let pt = self.saturated_point(Prop::SpeedOfSound)?;
        let w2 =
            self.defined(Prop::SpeedOfSound, relations::speed_of_sound_squared(self.r, pt.t, self.molar_mass, &pt.b))?;
        if w2 > 0.0 { Ok(math::sqrt(w2)) } else { Err(DomainError::MechanicallyUnstable.into()) }
    }
    /// A first partial derivative from the stored order-2 bundle: every family, no model call (E1); a saturated phase's
    /// at Q = 0 and 1, undefined strictly inside the dome. One of c_v, c_p and w needs the model's order-3 bundle, and one
    /// of τ and δ its reducing state: here they are `NoModel`, and [`crate::Fluid::prop`] gives them.
    pub fn partial(&self, p: Partial) -> Result<f64, Error> {
        self.partial_with(p, None, None)
    }

    /// [`State::partial`] with `total`, the model's order-3 bundle at this state as one phase (`ThermoModel::derivs`),
    /// which c_v's, c_p's and w's derivatives read, and the reducing state of τ and δ; `NoModel` where a variable's is
    /// `None`.
    pub(crate) fn partial_with(
        &self,
        p: Partial,
        total: Option<&Derivs>,
        reducing: Option<Reducing>,
    ) -> Result<f64, Error> {
        let prop = Prop::Partial(p);
        let pt = self.saturated_point(prop)?;
        let vars = [p.of, p.wrt, p.at];
        let missing = |v: &DerivVar| (v.is_second_order() && total.is_none()) || (v.is_reduced() && reducing.is_none());
        if vars.iter().any(missing) {
            return Err(Error::NoModel { prop });
        }
        let at = At { third: total, reducing, ..self.at(pt, &pt.b) };
        relations::first_partial(p.of, p.wrt, p.at, &at).ok_or(Error::Undefined { prop, phase: self.phase })
    }

    /// The derivative engine's view of the phase point `pt`, with the order-2 bundle `b`: no order-3 total, no reducing
    /// state.
    fn at<'a>(&self, pt: &Point, b: &'a Bundle) -> At<'a> {
        let (r, molar_mass, ds) = (self.r, self.molar_mass, self.gauge.ds());
        At { r, t: pt.t, rho: pt.rho, molar_mass, ds, b, third: None, reducing: None }
    }

    /// The isothermal compressibility κ_T = (1/ρ)(∂ρ/∂p)_T, 1/Pa, from the stored order-2 bundle; a saturated phase's at
    /// Q = 0 and 1, undefined strictly inside the dome.
    pub fn isothermal_compressibility(&self) -> Result<f64, Error> {
        let p = Partial { of: DerivVar::Dmolar, wrt: DerivVar::P, at: DerivVar::T };
        self.per_density(Prop::IsothermalCompressibility, p, 1.0)
    }

    /// The isobaric expansion coefficient β = −(1/ρ)(∂ρ/∂T)_p, 1/K, from the stored order-2 bundle; a saturated phase's
    /// at Q = 0 and 1, undefined strictly inside the dome.
    pub fn isobaric_expansion(&self) -> Result<f64, Error> {
        let p = Partial { of: DerivVar::Dmolar, wrt: DerivVar::T, at: DerivVar::P };
        self.per_density(Prop::IsobaricExpansion, p, -1.0)
    }

    /// `sign·p/ρ` for the first partial `p` of the density, as `prop`.
    fn per_density(&self, prop: Prop, p: Partial, sign: f64) -> Result<f64, Error> {
        let pt = self.saturated_point(prop)?;
        let at = self.at(pt, &pt.b);
        let v = relations::first_partial(p.of, p.wrt, p.at, &at).ok_or(Error::Undefined { prop, phase: self.phase })?;
        Ok(sign * v / pt.rho)
    }

    /// A second partial derivative from `total`, the model's order-3 bundle at this state (`ThermoModel::derivs`,
    /// E1), at Q = 0 and 1 at its saturated phase; undefined strictly inside the dome and where the derivative is (a
    /// degenerate pair, a vanishing Jacobian). c_v, c_p and w are its outer variables only, and τ and δ need the
    /// reducing state: `NoModel` (as [`State::partial`]; [`crate::Fluid::prop`] gives τ and δ).
    pub fn second_partial(&self, p: SecondPartial, total: &Derivs) -> Result<f64, Error> {
        self.second_partial_with(p, total, None)
    }

    /// [`State::second_partial`] with the reducing state of τ and δ.
    pub(crate) fn second_partial_with(
        &self,
        p: SecondPartial,
        total: &Derivs,
        reducing: Option<Reducing>,
    ) -> Result<f64, Error> {
        let prop = Prop::SecondPartial(p);
        let (pt, b) = (self.saturated_point(prop)?, total.bundle().ok_or(Error::NoModel { prop })?);
        let first = (p.first.of, p.first.wrt, p.first.at);
        // The inner partial is differentiated twice: c_v, c_p and w there would need order 4.
        let order_four = [first.0, first.1, first.2].iter().any(|v| v.is_second_order());
        let reduced = [first.0, first.1, first.2, p.wrt, p.at].iter().any(|v| v.is_reduced());
        if order_four || (reduced && reducing.is_none()) {
            return Err(Error::NoModel { prop });
        }
        let at = At { third: Some(total), reducing, ..self.at(pt, &b) };
        relations::second_partial(first, (p.wrt, p.at), &at, total).ok_or(Error::Undefined { prop, phase: self.phase })
    }

    /// The fundamental derivative of gas dynamics Γ = 1 + ρ(∂²p/∂ρ²)_s / (2(∂p/∂ρ)_s) from `total`, the model's
    /// order-3 bundle at this state, at Q = 0 and 1 at its saturated phase; undefined strictly inside the dome, and
    /// mechanically unstable where (∂p/∂ρ)_s ≤ 0, as the speed of sound is.
    pub fn fundamental_derivative(&self, total: &Derivs) -> Result<f64, Error> {
        let prop = Prop::FundamentalDerivative;
        let (pt, b) = (self.saturated_point(prop)?, total.bundle().ok_or(Error::NoModel { prop })?);
        let at = self.at(pt, &b);
        let (p, rho, s) = (DerivVar::P, DerivVar::Dmolar, DerivVar::Smolar);
        let undefined = || Error::Undefined { prop, phase: self.phase };
        let w2 = relations::first_partial(p, rho, s, &at).ok_or_else(undefined)?;
        if w2 > 0.0 {
            let curvature = relations::second_partial((p, rho, s), (rho, s), &at, total).ok_or_else(undefined)?;
            Ok(1.0 + pt.rho * curvature / (2.0 * w2))
        } else {
            Err(DomainError::MechanicallyUnstable.into())
        }
    }

    /// (T, ρ) of the single phase point (what `ThermoModel::derivs` evaluates at); `None` in the dome.
    pub(crate) fn single_t_rho(&self) -> Option<(f64, f64)> {
        match self.body {
            Body::Single(pt) => Some((pt.t, pt.rho)),
            Body::TwoPhase { .. } => None,
        }
    }

    /// This state as one phase, the one transport and the model's `derivs` read (ROT-010, M8.10, SQ2, SQ3): a single
    /// phase's own; at Q = 0 or 1 a split's saturated liquid or vapour, the single phase it was built from, its p the
    /// EOS's at its T and ρ; `None` strictly inside the dome.
    pub(crate) fn one_phase(&self) -> Option<State> {
        let side = |pt: Point, phase| {
            let p = relations::pressure(self.r, pt.t, pt.rho, &pt.b);
            // A split's M at q = 0 and 1 is that side's exactly ((1 − q)M′ + qM″ there), so `..*self` carries it.
            State { t: pt.t, p, phase, body: Body::Single(pt), ..*self }
        };
        match self.body {
            Body::Single(_) => Some(*self),
            Body::TwoPhase { q: 0.0, liquid, .. } => Some(side(liquid, Phase::Liquid)),
            Body::TwoPhase { q: 1.0, vapour, .. } => Some(side(vapour, Phase::Gas)),
            Body::TwoPhase { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivs::Order;

    const KEY: ModelKey = ModelKey::from_content(b"test");
    const R: f64 = 8.314_462_618;

    /// M7.10: where (∂p/∂ρ)_s is exactly 0 (A01 = A11 = 1, A02 = −2: (∂p/∂ρ)_T and (∂p/∂T)_ρ vanish) the fundamental
    /// derivative is refused as mechanically unstable, as w is, not divided by 0; κ_T is not finite there either.
    #[test]
    fn fundamental_derivative_needs_a_positive_speed_of_sound() {
        let total = Derivs::from_fn(Order::Three, |i, j| match (i, j) {
            (0, 1) | (1, 1) => 1.0,
            (2, 0) => -1.0,
            (0, 2) => -2.0,
            _ => 0.0,
        });
        let s = State::from_total(KEY, 300.0, 1_000.0, R, 0.018, Phase::Gas, &total.bundle().unwrap()).unwrap();
        assert_eq!(s.fundamental_derivative(&total), Err(DomainError::MechanicallyUnstable.into()));
        assert_eq!(s.speed_of_sound(), Err(DomainError::MechanicallyUnstable.into()));
    }

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

    /// g = RT·(A00 + A01) and a = RT·A00 in the native gauge; a gauge (Δh, Δs) shifts both by Δh − TΔs, as it shifts
    /// h − Ts and u − Ts; mass values are the molar ones over M.
    #[test]
    fn gibbs_and_helmholtz_energies_shift_with_the_gauge() {
        let b = Bundle { a00: -0.4, a10: 1.5, a01: 0.9, a20: -1.5, a11: 0.1, a02: -0.7 };
        let (t, m) = (300.0, 0.04);
        let s = State::from_total(KEY, t, 40.0, R, m, Phase::Gas, &b).unwrap();
        assert_eq!(s.g(Basis::Molar), R * t * (b.a00 + b.a01));
        assert_eq!(s.a(Basis::Molar), R * t * b.a00);
        let gauged = s.with_gauge(Gauge::new(1_000.0, 2.0).unwrap());
        let shift = 1_000.0 - t * 2.0;
        for (shifted, native) in
            [(gauged.g(Basis::Molar), s.g(Basis::Molar)), (gauged.a(Basis::Molar), s.a(Basis::Molar))]
        {
            assert!((shifted - native - shift).abs() < 1e-12 * shift, "{shifted} {native}");
        }
        let g = gauged.g(Basis::Molar);
        assert!((gauged.g(Basis::Mass) - g / m).abs() < 1e-15 * (g / m).abs());
        assert!((gauged.a(Basis::Mass) - gauged.a(Basis::Molar) / m).abs() < 1e-15 * (g / m).abs());
        assert!(
            (gauged.g(Basis::Molar) - (gauged.h(Basis::Molar) - t * gauged.s(Basis::Molar))).abs() < 1e-12 * g.abs()
        );
    }

    /// ROT-010, DIV-0004 (c_p, c_v part; map 10 R18): strictly inside the dome cv, cp, w and every partial are
    /// `Undefined { prop, TwoPhase }`, where CoolProp returns numbers (Water QT(0.5, 400 K): c_p 4056.47 J/kg/K); h, s,
    /// u and ρ are the quality-weighted mixture.
    #[test]
    fn two_phase_cp_cv_w_are_undefined() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let gas = State::from_total(KEY, 300.0, 40.0, R, 0.04, Phase::Gas, &b).unwrap();
        let liquid = State::from_total(KEY, 300.0, 4_000.0, R, 0.04, Phase::Liquid, &b).unwrap();
        let s = State::from_split(liquid, gas, Quality::new(0.5).unwrap(), 300.0, gas.p()).unwrap();
        let undefined = |prop| Err(Error::Undefined { prop, phase: Phase::TwoPhase });
        assert_eq!(s.cv(Basis::Molar), undefined(Prop::Cvmolar));
        assert_eq!(s.cp(Basis::Mass), undefined(Prop::Cpmolar));
        assert_eq!(s.speed_of_sound(), undefined(Prop::SpeedOfSound));
        let partial =
            Partial { of: crate::prop::DerivVar::Hmolar, wrt: crate::prop::DerivVar::T, at: crate::prop::DerivVar::P };
        assert_eq!(s.partial(partial), undefined(Prop::Partial(partial)));
        assert_eq!(s.h(Basis::Molar), (liquid.h(Basis::Molar) + gas.h(Basis::Molar)) / 2.0);
        assert_eq!(s.rho(Basis::Molar), 1.0 / ((1.0 / 4_000.0 + 1.0 / 40.0) / 2.0));
    }

    /// SQ1 (ROT-010, DIV-0004): c_p, c_v and w read a saturated phase as a single one, as CoolProp does and as transport
    /// does (M8.10): at Q = 0 a split's are its liquid's, at Q = 1 its vapour's, w at the vapour's own T (E3). Strictly
    /// inside the dome they stay `Undefined`.
    #[test]
    fn saturated_cp_cv_w_are_the_saturated_phase_s() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let gas = State::from_total(KEY, 301.0, 40.0, R, 0.04, Phase::Gas, &b).unwrap();
        let liquid =
            State::from_total(KEY, 300.0, 4_000.0, R, 0.04, Phase::Liquid, &Bundle { a20: -2.5, ..b }).unwrap();
        let split = |q| State::from_split(liquid, gas, Quality::new(q).unwrap(), 300.0, 1.5 * gas.p()).unwrap();
        let caloric = |s: &State| (s.cp(Basis::Mass), s.cv(Basis::Molar), s.speed_of_sound());
        assert_ne!(caloric(&liquid), caloric(&gas));
        assert_eq!(caloric(&split(0.0)), caloric(&liquid));
        assert_eq!(caloric(&split(1.0)), caloric(&gas));
        let undefined = |prop| Err(Error::Undefined { prop, phase: Phase::TwoPhase });
        let refused = (undefined(Prop::Cpmolar), undefined(Prop::Cvmolar), undefined(Prop::SpeedOfSound));
        assert_eq!((caloric(&split(1e-300)), caloric(&split(0.5))), (refused.clone(), refused));
    }

    /// SQ2 (ROT-010, DIV-0004): the partials, κ_T, β, the second partials and Γ read the point c_p, c_v and w read: at
    /// Q = 0 a split's liquid, at Q = 1 its vapour at its own T (E3), each from the order-3 bundle at that point.
    /// Strictly inside the dome they stay `Undefined`.
    #[test]
    fn saturated_partials_are_the_saturated_phase_s() {
        use DerivVar::{Dmolar, Hmolar, P, Smolar, T};
        // Ideal-gas-like bundles (A01 = 1, A02 = −1, A03 = 2), so (∂p/∂ρ)_s > 0 and Γ is defined.
        let total = |a20: f64| {
            Derivs::from_fn(Order::Three, move |i, j| match (i, j) {
                (1, 0) => 1.5,
                (0, 1) => 1.0,
                (2, 0) => a20,
                (0, 2) => -1.0,
                (3, 0) => -2.0 * a20,
                (0, 3) => 2.0,
                _ => 0.0,
            })
        };
        let (dense, dilute) = (total(-2.5), total(-1.5));
        let gas = State::from_total(KEY, 301.0, 40.0, R, 0.04, Phase::Gas, &dilute.bundle().unwrap()).unwrap();
        let liquid = State::from_total(KEY, 300.0, 4_000.0, R, 0.04, Phase::Liquid, &dense.bundle().unwrap()).unwrap();
        let split = |q| State::from_split(liquid, gas, Quality::new(q).unwrap(), 300.0, 1.5 * gas.p()).unwrap();
        let first = Partial { of: Hmolar, wrt: T, at: P };
        let second = SecondPartial { first: Partial { of: P, wrt: T, at: Dmolar }, wrt: Smolar, at: P };
        let outputs = |s: &State, d: &Derivs| {
            let (kappa, beta) = (s.isothermal_compressibility(), s.isobaric_expansion());
            [s.partial(first), kappa, beta, s.second_partial(second, d), s.fundamental_derivative(d)]
        };
        let (of_liquid, of_gas) = (outputs(&liquid, &dense), outputs(&gas, &dilute));
        assert!(of_liquid.iter().chain(&of_gas).all(Result::is_ok) && of_liquid != of_gas, "{of_liquid:?} {of_gas:?}");
        assert_eq!((outputs(&split(0.0), &dense), outputs(&split(1.0), &dilute)), (of_liquid, of_gas));
        let undefined = |prop| Err(Error::Undefined { prop, phase: Phase::TwoPhase });
        let refused = [
            undefined(Prop::Partial(first)),
            undefined(Prop::IsothermalCompressibility),
            undefined(Prop::IsobaricExpansion),
            undefined(Prop::SecondPartial(second)),
            undefined(Prop::FundamentalDerivative),
        ];
        assert_eq!((outputs(&split(1e-300), &dense), outputs(&split(0.5), &dense)), (refused.clone(), refused));
    }

    /// M20.1: c_v, c_p and w as derivative variables read the model's order-3 total, and τ and δ its reducing state, so a
    /// state alone refuses their partials as `NoModel`; a second partial refuses c_v, c_p and w inside its inner partial
    /// (order 4) whatever it is given. On a monatomic ideal gas (α⁰ = ln δ + 1.5 ln τ: A20 = −1.5, A30 = 3) c_v and c_p
    /// are constant and w ∝ √T, τ = T_r/T and δ = ρ/ρ_r.
    #[test]
    fn second_order_and_reduced_variables_need_the_model() {
        use DerivVar::{Cpmass, Cvmolar, Delta, Dmolar, P, SpeedOfSound, T, Tau};
        let total = Derivs::from_fn(Order::Three, |i, j| match (i, j) {
            (1, 0) => 1.5,
            (0, 1) => 1.0,
            (2, 0) => -1.5,
            (0, 2) => -1.0,
            (3, 0) => 3.0,
            (0, 3) => 2.0,
            _ => 0.0,
        });
        let (t, rho, reducing) = (300.0, 40.0, Reducing { t: 150.0, rho: 10_000.0, p: None });
        let s = State::from_total(KEY, t, rho, R, 0.04, Phase::Gas, &total.bundle().unwrap()).unwrap();
        let partial = |of, wrt, at| Partial { of, wrt, at };
        for p in [partial(Cvmolar, T, Dmolar), partial(P, SpeedOfSound, T), partial(T, P, Tau), partial(Delta, T, P)] {
            assert_eq!(s.partial(p), Err(Error::NoModel { prop: Prop::Partial(p) }), "{p:?}");
        }
        let with = |p| s.partial_with(p, Some(&total), Some(reducing)).unwrap();
        assert_eq!([with(partial(Cvmolar, T, Dmolar)), with(partial(Cpmass, Dmolar, T))], [0.0; 2]);
        let w = s.speed_of_sound().unwrap();
        assert!((with(partial(SpeedOfSound, T, Dmolar)) - w / (2.0 * t)).abs() < 1e-15 * w / t);
        assert_eq!([with(partial(Tau, T, Dmolar)), with(partial(Delta, Dmolar, T))], [-150.0 / (t * t), 1e-4]);
        let second = |first, wrt, at| SecondPartial { first, wrt, at };
        let (inner, reduced) = (second(partial(Cvmolar, T, P), T, P), second(partial(P, Tau, Delta), T, Dmolar));
        for p in [inner, reduced] {
            assert_eq!(s.second_partial(p, &total), Err(Error::NoModel { prop: Prop::SecondPartial(p) }), "{p:?}");
        }
        let with = |p| s.second_partial_with(p, &total, Some(reducing));
        assert_eq!(with(inner), Err(Error::NoModel { prop: Prop::SecondPartial(inner) }));
        // ∂(∂p/∂T)_ρ/∂T = 0 at constant ρ, and (∂p/∂τ)_δ = −ρRT²/T_r differentiated in T: −2ρRT/T_r.
        assert_eq!(s.second_partial(second(partial(P, T, Dmolar), SpeedOfSound, Dmolar), &total), Ok(0.0));
        let by_t = with(reduced).unwrap();
        assert!((by_t + 2.0 * rho * R * t / 150.0).abs() < 1e-13 * by_t.abs(), "{by_t}");
    }

    /// ROT-010 (M8.10): a saturated phase is a single phase. At Q = 0 a split's one phase is its liquid, at Q = 1 its
    /// vapour, each the state it was built from, at its own T (a pseudo-pure fluid's dew point is not at its bubble
    /// point's, E3); strictly inside the dome there is none. A single phase is its own.
    #[test]
    fn one_phase_is_the_saturated_side_at_q_0_and_1() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let gas = State::from_total(KEY, 301.0, 40.0, R, 0.04, Phase::Gas, &b).unwrap();
        let liquid = State::from_total(KEY, 300.0, 4_000.0, R, 0.04, Phase::Liquid, &b).unwrap();
        let split = |q| State::from_split(liquid, gas, Quality::new(q).unwrap(), 300.0, 1.5 * gas.p()).unwrap();
        assert_eq!(split(0.0).one_phase(), Some(liquid));
        assert_eq!(split(1.0).one_phase(), Some(gas));
        assert_eq!(split(0.5).one_phase(), None);
        assert_eq!(split(1e-300).one_phase(), None);
        assert_eq!(gas.one_phase(), Some(gas));
    }

    /// A split is extrapolated if either phase is, its vapour's alone included, and carries its own solve's path, not
    /// its liquid's.
    #[test]
    fn a_split_keeps_its_path_and_either_phase_s_extrapolation() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let gas = State::from_total(KEY, 300.0, 40.0, R, 0.04, Phase::Gas, &b).unwrap().mark_extrapolated();
        let liquid = State::from_total(KEY, 300.0, 4_000.0, R, 0.04, Phase::Liquid, &b).unwrap();
        let path = SolvePath { strategy: Strategy::Vle, iterations: 3 };
        let split = State::split(liquid, gas, Quality::new(0.5).unwrap(), 300.0, gas.p(), path).unwrap();
        assert_eq!((split.path(), split.is_extrapolated()), (path, true));
    }

    /// ROT-013 (map 01 U4): a single phase has no quality, `None` rather than CoolProp's −1 sentinel; a split's is its
    /// value.
    #[test]
    fn single_phase_quality_is_undefined() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let gas = State::from_total(KEY, 300.0, 40.0, R, 0.04, Phase::Gas, &b).unwrap();
        assert_eq!(gas.quality(), None);
        let liquid = State::from_total(KEY, 300.0, 4_000.0, R, 0.04, Phase::Liquid, &b).unwrap();
        let split = State::from_split(liquid, gas, Quality::new(0.25).unwrap(), 300.0, gas.p()).unwrap();
        assert_eq!(split.quality(), Some(0.25));
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

    /// M13.1: a mixture's phases differ in molar mass. A split weighs them by mass: its mass quality is qM″/((1 − q)M′ +
    /// qM″), its mass-basis values divide by that M, and each saturated side keeps its own M. It still refuses phases
    /// of different R.
    #[test]
    fn a_split_weighs_phases_of_different_molar_mass_by_mass() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let (m_l, m_v, q) = (0.05, 0.03, 0.25);
        let liquid = State::from_total(KEY, 300.0, 4_000.0, R, m_l, Phase::Liquid, &b).unwrap();
        let gas = State::from_total(KEY, 300.0, 40.0, R, m_v, Phase::Gas, &b).unwrap();
        let split = |q| State::from_split(liquid, gas, Quality::new(q).unwrap(), 300.0, gas.p());
        let Ok(mixed) = split(q) else { panic!("{:?}", split(q)) };
        let m = (1.0 - q) * m_l + q * m_v;
        let close = |a: f64, b: f64| ((a - b) / b).abs() < 1e-15;
        assert!(mixed.mass_quality().is_some_and(|x| close(x, q * m_v / m)), "{:?}", mixed.mass_quality());
        assert!(close(mixed.h(Basis::Mass), mixed.h(Basis::Molar) / m));
        assert!(close(mixed.s(Basis::Mass), mixed.s(Basis::Molar) / m));
        assert!(close(mixed.rho(Basis::Mass), m / ((1.0 - q) / 4_000.0 + q / 40.0)));
        let (bubble, dew) = (split(0.0).unwrap(), split(1.0).unwrap());
        assert_eq!((bubble.one_phase(), dew.one_phase()), (Some(liquid), Some(gas)));
        assert_eq!(bubble.cp(Basis::Mass), liquid.cp(Basis::Mass));
        assert_eq!(dew.speed_of_sound(), gas.speed_of_sound());
        assert_eq!((bubble.mass_quality(), dew.mass_quality()), (Some(0.0), Some(1.0)));
        let other_r = State::from_total(KEY, 300.0, 40.0, 2.0 * R, m_v, Phase::Gas, &b).unwrap();
        let refused = State::from_split(liquid, other_r, Quality::new(q).unwrap(), 300.0, gas.p());
        assert_eq!(refused, Err(Error::InvalidState { reason: "the phases of a split must share R" }));
    }

    /// M13.1: phases of one molar mass (a pure fluid's) keep every bit: M is not recomputed from the lever rule, and the
    /// mass quality is the molar one.
    #[test]
    fn a_split_of_one_molar_mass_keeps_its_bits() {
        let b = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let m = 0.044_097;
        let liquid = State::from_total(KEY, 300.0, 4_000.0, R, m, Phase::Liquid, &b).unwrap();
        let gas = State::from_total(KEY, 300.0, 40.0, R, m, Phase::Gas, &b).unwrap();
        for q in [0.1, 0.3, 0.7, 0.9] {
            let split = State::from_split(liquid, gas, Quality::new(q).unwrap(), 300.0, gas.p()).unwrap();
            assert_eq!(split.mass_quality(), Some(q));
            let h = ((1.0 - q) * relations::enthalpy(R, 300.0, &b) + q * relations::enthalpy(R, 300.0, &b) + 0.0)
                * (1.0 / m);
            assert_eq!(split.h(Basis::Mass).to_bits(), h.to_bits(), "{q}");
        }
    }
}
