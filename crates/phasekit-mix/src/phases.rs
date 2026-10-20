//! The phases of a mixture's two-phase state (PLAN.md M21.5). Core's `State` keeps each phase's density and molar mass
//! but not its mole fractions, so they are found again from the state's T, p and molar quality β by
//! [`Mixture::coexisting`], the solve the QT and PQ flashes run: at the state's T and at its p, keeping the one that
//! lands nearer the state. The two differ where a T (or a p) has two splits at β, as on the dew side between the
//! critical temperature and the cricondentherm, where the solve at T may find the other one.

use phasekit_core::{Error, State, ThermoModel};

use crate::Mixture;
use crate::saturation::{Coexisting, Given};

impl Mixture {
    /// The two phases of `state`, a two-phase state of this mixture, at its molar quality β: the solve at its T, or at
    /// its p, whichever lands nearer the state's other coordinate (relatively), the one at T where they land equally
    /// near; the one that converged where the other did not. `None` for a single phase; `ForeignState` for a state of
    /// another model. Where neither solve converges, the refusal of the one at T.
    pub fn phases(&self, state: &State) -> Result<Option<Coexisting>, Error> {
        if state.key() != self.info().key() {
            return Err(Error::ForeignState);
        }
        let Some(beta) = state.quality() else {
            return Ok(None);
        };
        let (t, p) = (state.t(), state.p());
        let at_t = self.coexisting(beta, Given::T(t));
        let at_p = self.coexisting(beta, Given::P(p));
        let scored = [at_t.as_ref().ok().map(|c| (miss(c.p, p), c)), at_p.as_ref().ok().map(|c| (miss(c.t, t), c))];
        // `min_by` keeps the first of equal minima: the solve at T.
        match scored.into_iter().flatten().min_by(|a, b| a.0.total_cmp(&b.0)) {
            Some((_, c)) => Ok(Some(c.clone())),
            None => at_t.map(Some),
        }
    }
}

/// How far `found` lands from the state's `value`, relatively.
fn miss(found: f64, value: f64) -> f64 {
    (found / value - 1.0).abs()
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use phasekit_core::{Basis, DataSet, FlashOptions, Fluid, Input, Pair, Quality, Registry};

    use super::*;
    use crate::with_mixtures;

    /// The mixture `name` names, its fluid and its model.
    fn fluid(name: &str) -> Fluid {
        let plain = Registry::from_embedded(DataSet::Corrected).unwrap_or_else(|e| panic!("{e:?}"));
        with_mixtures(&plain, DataSet::Corrected).resolve(name).unwrap_or_else(|e| panic!("{name}: {e:?}"))
    }

    fn model(fluid: &Fluid) -> &Mixture {
        (fluid.model() as &dyn Any).downcast_ref::<Mixture>().unwrap_or_else(|| panic!("not a mixture"))
    }

    fn flash(fluid: &Fluid, pair: Pair, x: f64, y: f64) -> State {
        let input = Input::new(pair, x, y, Basis::Molar).unwrap_or_else(|e| panic!("{e:?}"));
        fluid.flash(input, &FlashOptions::new()).unwrap_or_else(|e| panic!("{pair:?} {x} {y}: {e:?}"))
    }

    fn found(mixture: &Mixture, state: &State) -> Coexisting {
        mixture.phases(state).unwrap_or_else(|e| panic!("{e:?}")).unwrap_or_else(|| panic!("no phases"))
    }

    /// A QT state's phases are the QT flash's own solve, bit for bit: its liquid's and vapour's mole fractions, which add
    /// up to the feed at β; at Q = 0 the liquid is the feed and the vapour the incipient phase.
    #[test]
    fn a_qt_states_phases_are_its_flashs_solve() {
        let fluid = fluid("R32[0.4]&R125[0.6]");
        let mixture = model(&fluid);
        for (q, t) in [(0.5, 250.0), (0.0, 260.0), (1.0, 240.0)] {
            let state = flash(&fluid, Pair::QT, q, t);
            let want = mixture.coexisting(q, Given::T(t)).unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(found(mixture, &state), want, "{q} {t}");
            for (i, z) in mixture.composition().iter().enumerate() {
                let sum = (1.0 - q) * want.x[i] + q * want.y[i];
                assert!((sum - z).abs() <= 1e-12, "{q} {t}: component {i}: {sum} against {z}");
            }
        }
        let bubble = found(mixture, &flash(&fluid, Pair::QT, 0.0, 260.0));
        assert_eq!(bubble.x, mixture.composition());
        assert_ne!(bubble.y, mixture.composition());
    }

    /// On methane/ethane's retrograde dew side, 212 K has two dew points, at 4.41 and 5.87 MPa, and the solve at T finds
    /// the lower; a dew state at the upper pressure takes the solve at its p, which lands on the state.
    #[test]
    fn a_retrograde_dew_state_takes_the_solve_at_its_pressure() {
        let fluid = fluid("Methane[0.9]&Ethane[0.1]");
        let mixture = model(&fluid);
        let p = 5.874e6;
        let state = flash(&fluid, Pair::PQ, p, 1.0);
        let at_t = mixture.coexisting(1.0, Given::T(state.t())).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(at_t.p < 4.6e6, "the solve at T lands on the other dew point: {}", at_t.p);
        let want = mixture.coexisting(1.0, Given::P(p)).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(found(mixture, &state), want);
    }

    /// A single phase has none; a state of another model is refused; a split neither solve reaches (a hand-made one at
    /// 1000 K and 1 GPa, above the dome) is refused as the solve at T refuses it.
    #[test]
    fn a_single_phase_has_none_and_a_foreign_state_is_refused() {
        let fluid = fluid("R32[0.4]&R125[0.6]");
        let mixture = model(&fluid);
        let gas = flash(&fluid, Pair::PT, 1e5, 300.0);
        assert_eq!(mixture.phases(&gas), Ok(None));
        let other = self::fluid("CarbonDioxide[0.5]&Nitrogen[0.5]");
        let foreign = flash(&other, Pair::PT, 1e5, 300.0);
        assert_eq!(mixture.phases(&foreign), Err(Error::ForeignState));
        let liquid = flash(&fluid, Pair::PT, 5e6, 250.0);
        let half = Quality::new(0.5).unwrap_or_else(|e| panic!("{e:?}"));
        let split = State::from_split(liquid, gas, half, 1000.0, 1e9).unwrap_or_else(|e| panic!("{e:?}"));
        let refusal = mixture.coexisting(0.5, Given::T(1000.0)).err();
        assert!(refusal.is_some());
        assert_eq!(mixture.phases(&split).err(), refusal);
    }
}
