//! L4 the Helmholtz flash (D6): a pure function of an immutable package, a native input and per-call
//! options. No global configuration, no sticky phase, no exception cascade (map 01 R12, map 03 §6, map 12
//! R3/R4). Each pair runs a fixed, ordered list of strategies returning `Result`; the winner is recorded in
//! `State::path`. The sketch implements DT; the other pairs land M5-M7 behind `IMPLEMENTED`.
#![deny(clippy::indexing_slicing)] // E12: no panicking index on the flash path

use crate::derivs::{Bundle, Order};
use crate::error::Error;
use crate::fluid::PureFluid;
use crate::input::{NativeInput, Pair};
use crate::model::ThermoModel;
use crate::relations;
use crate::saturation::{SatAccuracy, SatPair};
use crate::state::{Phase, SolvePath, State, Strategy};
use crate::units::Quality;

/// How to choose among several roots of a non-unique pair (T+H in compressed liquid, Q-pairs with 2-3 roots;
/// map 03 §6). The default never guesses.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum RootPolicy {
    /// Several roots → `Error::Ambiguous { roots }`.
    #[default]
    Strict,
    /// The root nearest to this value of the solved variable.
    Nearest(f64),
    /// The thermodynamically stable root (lowest Gibbs energy).
    Stable,
}

/// Whether the validity domain is enforced, per call. Replaces CoolProp's global `DONT_CHECK_PROPERTY_LIMITS`
/// (map 03 §6). Under either policy a saturation fit (superancillary, ancillary) is never evaluated outside
/// its fitted range (D6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DomainPolicy {
    /// Refuse states below the triple point or `Tmin`, above `Tmax` or `pmax`, or below the melting line.
    #[default]
    Enforce,
    /// Evaluate the EOS there anyway, for metastable single-phase states (supercooled liquid water). The
    /// state is flagged (`State::is_extrapolated`). Where the phase rule would need a fit outside its range,
    /// the phase comes from a single-phase hint or the generic VLE (M6).
    Extrapolate,
}

/// Per-call options, passed by value. Private fields and `const` builders keep them growable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlashOptions {
    phase_hint: Option<Phase>,
    roots: RootPolicy,
    domain: DomainPolicy,
    guess: Option<(f64, f64)>,
}

impl FlashOptions {
    /// CoolProp-equivalent defaults: no hint, strict roots, domain enforced, cold start.
    pub const fn new() -> Self {
        Self { phase_hint: None, roots: RootPolicy::Strict, domain: DomainPolicy::Enforce, guess: None }
    }
    /// Imposes a phase for this call only (never sticky). A single-phase label skips the phase rule;
    /// `TwoPhase` requires the dome and is refused (quality outside [0, 1]) where the state is not in it.
    pub const fn with_phase(mut self, phase: Phase) -> Self {
        self.phase_hint = Some(phase);
        self
    }
    /// Root selection policy.
    pub const fn with_roots(mut self, roots: RootPolicy) -> Self {
        self.roots = roots;
        self
    }
    /// Domain policy.
    pub const fn with_domain(mut self, domain: DomainPolicy) -> Self {
        self.domain = domain;
        self
    }
    /// Warm start at (T, ρ_molar). Opt-in: results then depend on the guess (K7).
    pub const fn with_guess(mut self, t: f64, rho: f64) -> Self {
        self.guess = Some((t, rho));
        self
    }
    /// The phase hint.
    pub const fn phase_hint(&self) -> Option<Phase> {
        self.phase_hint
    }
    /// The root policy.
    pub const fn roots(&self) -> RootPolicy {
        self.roots
    }
    /// The domain policy.
    pub const fn domain(&self) -> DomainPolicy {
        self.domain
    }
    /// The warm-start guess, if any.
    pub const fn guess(&self) -> Option<(f64, f64)> {
        self.guess
    }
}

/// The flash of a [`PureFluid`]: dispatch on the pair of an already normalised input.
pub(crate) fn flash(fluid: &PureFluid, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
    let (x, y) = input.values();
    match input.pair() {
        Pair::DT => dt(fluid, y, x, opts),
        pair => Err(Error::Unsupported { pair }),
    }
}

/// Where a (T, ρ) point lies.
enum Region {
    Single(Phase),
    Dome(SatPair),
}

/// The one kernel-defined phase rule (map 01 R26): per-call hint, else the model's critical point, else the
/// saturation curve. Below Tc without an `Exact`/`Definition` curve whose fitted range covers T, the generic
/// VLE decides; until it lands (M6) that case is `Unsupported`, never a load error, a guess or a fit
/// evaluated outside its range (S-10, D6).
fn region(fluid: &PureFluid, t: f64, rho: f64, p: f64, opts: &FlashOptions) -> Result<Region, Error> {
    let hint = opts.phase_hint();
    if let Some(single) = hint.filter(|h| *h != Phase::TwoPhase) {
        return Ok(Region::Single(single));
    }
    let crit = fluid.critical_point();
    if hint.is_none() {
        if let Some(c) = crit.filter(|c| t >= c.t) {
            let phase = if t == c.t && rho == c.rho {
                Phase::CriticalPoint
            } else if p > c.p {
                Phase::Supercritical
            } else {
                Phase::SupercriticalGas
            };
            return Ok(Region::Single(phase));
        }
    }
    let unsupported = Error::Unsupported { pair: Pair::DT };
    let Some(curve) = fluid.saturation()? else { return Err(unsupported) };
    let (t_lo, t_hi) = curve.t_range();
    if curve.accuracy() == SatAccuracy::Guess || !(t_lo..=t_hi).contains(&t) {
        return Err(unsupported); // M6: a pure-fluid VLE solve (polishing a guess, or alone outside the fit)
    }
    let sat = curve.at_t(t)?;
    Ok(if hint == Some(Phase::TwoPhase) {
        Region::Dome(sat)
    } else if rho >= sat.bubble.rho {
        let above_pc = crit.is_some_and(|c| p > c.p);
        Region::Single(if above_pc { Phase::SupercriticalLiquid } else { Phase::Liquid })
    } else if rho <= sat.dew.rho {
        Region::Single(Phase::Gas)
    } else {
        Region::Dome(sat)
    })
}

/// Order-2 total bundle at (T, ρ): one ideal and one residual evaluation.
fn total(fluid: &PureFluid, t: f64, rho: f64) -> Result<Bundle, Error> {
    let eos = fluid.eos();
    let d = eos.ideal(t, rho, Order::Two) + eos.residual(t, rho, Order::Two);
    d.bundle().ok_or(Error::InvalidState { reason: "the model returned a bundle below the requested order 2" })
}

/// DT: no iteration. Domain check, bundle, phase rule, `State`; flagged if outside the domain (D6).
fn dt(fluid: &PureFluid, t: f64, rho: f64, opts: &FlashOptions) -> Result<State, Error> {
    let enforce = opts.domain() == DomainPolicy::Enforce;
    let t_check = fluid.limits().check_t(t);
    if enforce {
        t_check?;
    }
    let (r, m, key) = (fluid.eos().gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let path = SolvePath { strategy: Strategy::Direct, iterations: 0 };
    let b = total(fluid, t, rho)?;
    let p = relations::pressure(r, t, rho, &b);
    let p_check = fluid.limits().check_p(p);
    if enforce {
        p_check?;
    }
    let state = match region(fluid, t, rho, p, opts)? {
        Region::Single(phase) => State::single(key, t, rho, r, m, phase, &b, path),
        Region::Dome(sat) if sat.is_pure() => {
            let (rl, rv) = (sat.bubble.rho, sat.dew.rho);
            let liquid = State::single(key, t, rl, r, m, Phase::Liquid, &total(fluid, t, rl)?, path)?;
            let vapour = State::single(key, t, rv, r, m, Phase::Gas, &total(fluid, t, rv)?, path)?;
            // Lever rule on molar volume; an imposed TwoPhase outside the dome fails here (q ∉ [0, 1]).
            let q = Quality::new((1.0 / rho - 1.0 / rl) / (1.0 / rv - 1.0 / rl))?;
            State::split(liquid, vapour, q, t, sat.bubble.p, path)
        }
        // Pseudo-pure in-dome DT: CoolProp's rule (D4), specified from oracle fixtures at M6 (map 04 U4).
        Region::Dome(_) => Err(Error::Unsupported { pair: Pair::DT }),
    }?;
    // Under `Extrapolate`, a state outside the domain is returned, never silently: it carries the flag.
    Ok(if t_check.is_ok() && p_check.is_ok() { state } else { state.mark_extrapolated() })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::batch::{self, BatchRequest, Status};
    use crate::data::FluidRecord;
    use crate::error::DomainError;
    use crate::fluid::Fluid;
    use crate::input::Input;
    use crate::model::Limits;
    use crate::prop::Prop;
    use crate::saturation::{SatSide, SaturationCurve};
    use crate::units::{Basis, Density, Temperature};

    /// A curve fitted over 200-380 K that fails the test if it is ever evaluated outside that range.
    #[derive(Debug)]
    struct Fit;

    impl SaturationCurve for Fit {
        fn accuracy(&self) -> SatAccuracy {
            SatAccuracy::Exact
        }
        fn t_range(&self) -> (f64, f64) {
            (200.0, 380.0)
        }
        fn at_t(&self, t: f64) -> Result<SatPair, Error> {
            assert!((200.0..=380.0).contains(&t), "fit evaluated outside its range at {t} K");
            let side = |rho| SatSide { t, p: 1e5, rho };
            Ok(SatPair { bubble: side(4_000.0), dew: side(50.0) })
        }
        fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
            Err(Error::Unsupported { pair: Pair::PQ })
        }
    }

    /// D6: below the triple point the default refuses; `Extrapolate` evaluates a metastable single-phase
    /// state and flags it (scalar and batch); a saturation fit is never evaluated outside its range.
    #[test]
    fn extrapolation_is_opt_in_flagged_and_never_extends_a_fit() {
        let mut record = FluidRecord::toy("X").unwrap();
        record.limits = Limits::new(169.0, 420.0, 100e6).unwrap().with_t_triple(175.0);
        let fluid = Fluid::new(Arc::new(record.builder().unwrap().saturation(Fit).build()));
        let dt = |t| Input::dt(Density::molar(5_000.0).unwrap(), Temperature::new(t).unwrap());
        let enforce = FlashOptions::new();
        let extrapolate = enforce.with_domain(DomainPolicy::Extrapolate);
        let liquid = |opts: FlashOptions| opts.with_phase(Phase::Liquid);

        let below_triple = DomainError::BelowMinTemperature { t: 172.0, t_min: 175.0 };
        assert_eq!(fluid.flash(dt(172.0), &liquid(enforce)), Err(below_triple.into()));
        assert!(fluid.flash(dt(172.0), &liquid(extrapolate)).unwrap().is_extrapolated());
        assert!(!fluid.flash(dt(300.0), &liquid(extrapolate)).unwrap().is_extrapolated());
        // Without a hint the phase rule needs the fit; outside 200-380 K it is not evaluated under either policy.
        for (t, opts) in [(172.0, extrapolate), (190.0, enforce), (190.0, extrapolate)] {
            assert_eq!(fluid.flash(dt(t), &opts), Err(Error::Unsupported { pair: Pair::DT }));
        }
        assert_eq!(fluid.flash(dt(300.0), &enforce).unwrap().phase(), Phase::Liquid);

        // The flag survives the batch driver as a per-cell status that is not a failure.
        let (x, y) = ([5_000.0; 2], [172.0, 300.0]);
        let req = BatchRequest::new(Pair::DT, Basis::Molar, &x, &y, &[Prop::P]).with_flash(liquid(extrapolate));
        let (mut out, mut status) = ([0.0; 2], [Status::Ok; 2]);
        let summary = batch::evaluate(&fluid, &req, &mut out, &mut status).unwrap();
        assert_eq!((status, summary.failed_cells), ([Status::Extrapolated, Status::Ok], 0));
    }
}
