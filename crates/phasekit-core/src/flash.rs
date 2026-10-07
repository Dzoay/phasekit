//! L4 the Helmholtz flash (D6): a pure function of an immutable package, a native input and per-call
//! options. No global configuration, no sticky phase, no exception cascade (map 01 R12, map 03 §6, map 12
//! R3/R4). Each pair runs a fixed, ordered list of strategies returning `Result`; the winner is recorded in
//! `State::path`. The sketch implements DT; the other pairs land M5-M7 behind `IMPLEMENTED`.
#![deny(clippy::indexing_slicing)] // E12: no panicking index on the flash path

use crate::derivs::{Bundle, Order};
use crate::error::{DomainError, Error};
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
/// saturation curve. The critical point that labels states is the model's own: the top of an exact saturation curve
/// (CoolProp's with superancillaries on), else the published one; R114's published 418.83 K and 3.257 MPa sit 1.8 K and
/// 3 % below its EOS's (map 03 §3.3, measured at M5.3), and the dome reaches up to the EOS's. Below Tc without an
/// `Exact`/`Definition` curve whose fitted range covers T, the generic VLE decides; until it lands (M6) that case is
/// `Unsupported`, never a load error, a guess or a fit evaluated outside its range (S-10, D6).
fn region(fluid: &PureFluid, t: f64, rho: f64, p: f64, opts: &FlashOptions) -> Result<Region, Error> {
    let hint = opts.phase_hint();
    if let Some(single) = hint.filter(|h| *h != Phase::TwoPhase) {
        return Ok(Region::Single(single));
    }
    let curve = fluid.saturation()?.filter(|c| c.accuracy() != SatAccuracy::Guess);
    let crit = match curve.filter(|c| c.accuracy() == SatAccuracy::Exact) {
        Some(c) => {
            let t_c = c.t_range().1;
            let top = c.at_t(t_c)?;
            Some((t_c, top.bubble.p, top.bubble.rho))
        }
        None => fluid.critical_point().map(|c| (c.t, c.p, c.rho)),
    };
    if hint.is_none() {
        if let Some((t_c, p_c, rho_c)) = crit.filter(|c| t >= c.0) {
            let phase = if t == t_c && rho == rho_c {
                Phase::CriticalPoint
            } else if p > p_c {
                Phase::Supercritical
            } else {
                Phase::SupercriticalGas
            };
            return Ok(Region::Single(phase));
        }
    }
    let unsupported = Error::Unsupported { pair: Pair::DT };
    let Some(curve) = curve else { return Err(unsupported) };
    let (t_lo, t_hi) = curve.t_range();
    if !(t_lo..=t_hi).contains(&t) {
        return Err(unsupported); // M6: a pure-fluid VLE solve outside the fit
    }
    let sat = curve.at_t(t)?;
    Ok(if hint == Some(Phase::TwoPhase) {
        Region::Dome(sat)
    } else if rho >= sat.bubble.rho {
        let above_pc = crit.is_some_and(|c| p > c.1);
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

/// DT: no iteration. Temperature check, bundle, phase rule, `State`, then the pressure check on the state's own p (in
/// the dome the saturation pressure, not the single-phase EOS's, which can be 1e11 Pa there); flagged if outside the
/// domain (D6). A single-phase state the phase rule chose (no hint) must be mechanically stable: the acceptance gate
/// (D6; map 03 §6, `agent-notes.md`: dp/dρ > 0 and cv > 0).
fn dt(fluid: &PureFluid, t: f64, rho: f64, opts: &FlashOptions) -> Result<State, Error> {
    let enforce = opts.domain() == DomainPolicy::Enforce;
    let t_check = fluid.limits().check_t(t);
    if enforce {
        t_check?;
    }
    let (r, m, key) = (fluid.eos().gas_constant(), fluid.info().molar_mass(), fluid.info().key());
    let b = total(fluid, t, rho)?;
    let p = relations::pressure(r, t, rho, &b);
    let state = match region(fluid, t, rho, p, opts)? {
        Region::Single(phase) => {
            if opts.phase_hint().is_none() && phase != Phase::CriticalPoint {
                accept(r, t, &b)?;
            }
            State::single(key, t, rho, r, m, phase, &b, DIRECT)
        }
        Region::Dome(sat) if sat.is_pure() => {
            // The curve's equilibrium is the answer (ROT-092: no VLE when the superancillary is exact).
            let path = SolvePath { strategy: Strategy::Superancillary, iterations: 0 };
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
    let p_check = fluid.limits().check_p(state.p());
    if enforce {
        p_check?;
    }
    // Under `Extrapolate`, a state outside the domain is returned, never silently: it carries the flag.
    Ok(if t_check.is_ok() && p_check.is_ok() { state } else { state.mark_extrapolated() })
}

/// The no-iteration path.
const DIRECT: SolvePath = SolvePath { strategy: Strategy::Direct, iterations: 0 };

/// The acceptance gate for a single-phase state the flash chose: (∂p/∂ρ)_T > 0 and cv > 0 (D6); an infinite cv (a
/// non-analytic critical point) passes.
fn accept(r: f64, t: f64, b: &Bundle) -> Result<(), Error> {
    let stable = relations::dp_drho_t(r, t, b) > 0.0 && relations::cv(r, b) > 0.0;
    if stable { Ok(()) } else { Err(DomainError::MechanicallyUnstable.into()) }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::batch::{self, BatchRequest, Status};
    use crate::data::FluidRecord;
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
            let side = |rho| SatSide { t, p: 1e9, rho }; // the top sets the critical pressure: above every toy state
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
        let mut record = FluidRecord::synthetic("X").unwrap();
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

        // Real Water (PLAN.md M5.3): supercooled liquid at 260 K is refused by default, flagged under `Extrapolate`
        // with a liquid hint, and without a hint not evaluated at all, because the superancillary starts at 273.16 K.
        #[cfg(feature = "fluids-all")]
        {
            let water = embedded("Water");
            let input = Input::dt(Density::molar(55_000.0).unwrap(), Temperature::new(260.0).unwrap());
            let below = DomainError::BelowMinTemperature { t: 260.0, t_min: 273.16 };
            assert_eq!(water.flash(input, &enforce), Err(below.into()));
            let supercooled = water.flash(input, &liquid(extrapolate)).unwrap();
            assert!(supercooled.is_extrapolated() && supercooled.phase() == Phase::Liquid);
            assert_eq!(water.flash(input, &extrapolate), Err(Error::Unsupported { pair: Pair::DT }));
        }
    }

    /// `name` from a private registry over the embedded data (the process-wide one stays untouched: a registry test
    /// checks that nothing in it is loaded).
    #[cfg(feature = "fluids-all")]
    fn embedded(name: &str) -> Fluid {
        crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap().get(name).unwrap().clone()
    }

    /// The decoded record of `name`, from a private registry.
    #[cfg(feature = "fluids-all")]
    fn record_of(name: &str) -> FluidRecord {
        crate::internal::record(&crate::Registry::from_embedded(crate::DataSet::Corrected).unwrap(), name).unwrap()
    }

    #[cfg(feature = "fluids-all")]
    fn dt(rho: f64, t: f64) -> Input {
        Input::dt(Density::molar(rho).unwrap(), Temperature::new(t).unwrap())
    }

    /// ROT-078, DIV-0012 (map 12 §6.3, #3394): Water at 250 K, below its 273.16 K triple point, is refused; CoolProp
    /// accepts DT(55018.5 mol/m³, 250 K) and returns p = −5.93 Pa.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn dt_below_the_triple_point_is_refused() {
        let below = DomainError::BelowMinTemperature { t: 250.0, t_min: 273.16 };
        assert_eq!(embedded("Water").state(dt(55_018.5, 250.0)), Err(below.into()));
    }

    /// D6 (map 03 §6, map 01 R12): `Extrapolate` belongs to the call. The same handle refuses the same state on the
    /// next call with the default options, and a hint given once is not remembered.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn extrapolate_is_per_call_never_sticky() {
        let water = embedded("Water");
        let extrapolate = FlashOptions::new().with_phase(Phase::Liquid).with_domain(DomainPolicy::Extrapolate);
        let below = Err(DomainError::BelowMinTemperature { t: 265.0, t_min: 273.16 }.into());
        for _ in 0..2 {
            assert!(water.flash(dt(55_000.0, 265.0), &extrapolate).unwrap().is_extrapolated());
            assert_eq!(water.state(dt(55_000.0, 265.0)), below);
            assert_eq!(water.state(dt(55_400.0, 300.0)).unwrap().phase(), Phase::Liquid); // above ρ′ = 55,315
        }
    }

    /// User decision 4, map 04 U4: a pseudo-pure fluid has no saturation curve before M6 (its ancillaries land at
    /// M6.3), so below its critical temperature DT is `Unsupported`, in the dome and out of it; above, it answers.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn pseudo_pure_subcritical_dt_is_unsupported_until_m6() {
        let r410a = embedded("R410A");
        for rho in [100.0, 5_000.0, 15_000.0] {
            assert_eq!(r410a.state(dt(rho, 280.0)), Err(Error::Unsupported { pair: Pair::DT }), "{rho} mol/m³");
        }
        assert_eq!(r410a.state(dt(5_000.0, 400.0)).unwrap().phase(), Phase::Supercritical);
    }

    /// ROT-020: the state names the strategy that produced it: `Direct` for a single phase, `Superancillary` in the
    /// dome, where the curve's equilibrium is the answer.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn solve_path_names_the_winning_strategy() {
        let water = embedded("Water");
        let strategy = |rho, t| water.state(dt(rho, t)).unwrap().path();
        let direct = SolvePath { strategy: Strategy::Direct, iterations: 0 };
        assert_eq!([strategy(10.0, 700.0), strategy(10.0, 400.0), strategy(55_400.0, 300.0)], [direct; 3]);
        assert_eq!(strategy(1_000.0, 400.0), SolvePath { strategy: Strategy::Superancillary, iterations: 0 });
    }

    /// ROT-092 (map 03 §6): a DT state in the dome reads the exact curve, no VLE: its pressure and phase densities are
    /// the curve's, bit for bit. The pressure check uses that saturation pressure: at 30,000 mol/m³ and 400 K the
    /// single-phase EOS gives 2.2e11 Pa, far above pmax, yet the state is a valid two-phase one. A `Guess` curve
    /// would need a VLE polish (M6), so DT inside it is `Unsupported`.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn dt_in_dome_reads_the_curve_not_vle() {
        let state = embedded("Water").state(dt(30_000.0, 400.0)).unwrap();
        let sat = record_of("Water").superancillary_curve().unwrap().at_t(400.0).unwrap();
        assert_eq!((state.phase(), state.p().to_bits()), (Phase::TwoPhase, sat.bubble.p.to_bits()));
        let q = (1.0 / 30_000.0 - 1.0 / sat.bubble.rho) / (1.0 / sat.dew.rho - 1.0 / sat.bubble.rho);
        assert_eq!(state.quality().map(f64::to_bits), Some(q.to_bits()));

        #[derive(Debug)]
        struct Stale;
        impl SaturationCurve for Stale {
            fn accuracy(&self) -> SatAccuracy {
                SatAccuracy::Guess
            }
            fn t_range(&self) -> (f64, f64) {
                (200.0, 380.0)
            }
            fn at_t(&self, _t: f64) -> Result<SatPair, Error> {
                unreachable!("a Guess curve is never used as the answer")
            }
            fn at_p(&self, _p: f64) -> Result<SatPair, Error> {
                Err(Error::Unsupported { pair: Pair::PQ })
            }
        }
        let record = FluidRecord::synthetic("X").unwrap();
        let toy = Fluid::new(Arc::new(record.builder().unwrap().saturation(Stale).build()));
        assert_eq!(toy.state(dt(1_000.0, 300.0)), Err(Error::Unsupported { pair: Pair::DT }));
    }

    /// The model's critical point labels states (map 03 §3.3): R114's published point (418.83 K, 3.257 MPa) lies below
    /// its EOS's, the top of its superancillary (420.61 K, 3.352 MPa), so at 419.5 K the dome is still there: a state
    /// between its densities is two-phase, as CoolProp says with superancillaries on, not supercritical.
    #[cfg(feature = "fluids-all")]
    #[test]
    fn phase_labels_use_the_models_critical_point() {
        let record = record_of("R114");
        let published = record.critical.unwrap();
        let curve = record.superancillary_curve().unwrap();
        let (t, t_c) = (419.5, curve.t_range().1);
        assert!(published.t < t && t < t_c, "{} < {t} < {t_c}", published.t);
        let sat = curve.at_t(t).unwrap();
        let state = embedded("R114").state(dt((sat.bubble.rho + sat.dew.rho) / 2.0, t)).unwrap();
        assert_eq!(state.phase(), Phase::TwoPhase);
        assert_eq!(embedded("R114").state(dt(sat.dew.rho / 2.0, t)).unwrap().phase(), Phase::Gas);
    }

    /// The acceptance gate (D6): a single-phase state needs (∂p/∂ρ)_T > 0 and cv > 0; an infinite cv passes.
    #[test]
    fn acceptance_gate_needs_mechanical_stability() {
        let stable = Bundle { a00: 0.0, a10: 1.5, a01: 1.0, a20: -1.5, a11: 0.0, a02: -1.0 };
        let r = 8.314_462_618;
        assert_eq!(accept(r, 300.0, &stable), Ok(()));
        assert_eq!(accept(r, 300.0, &Bundle { a20: f64::NEG_INFINITY, ..stable }), Ok(()));
        let unstable = Err(DomainError::MechanicallyUnstable.into());
        assert_eq!(accept(r, 300.0, &Bundle { a02: -2.0, ..stable }), unstable); // (∂p/∂ρ)_T = 0
        assert_eq!(accept(r, 300.0, &Bundle { a20: 0.0, ..stable }), unstable); // cv = 0
    }
}
