//! QT and PQ on the saturation curve (PLAN.md M6.8).

use super::*;

/// ROT-004, ROT-085 (map 03 §6 row 4; PLAN.md M6.8): QT and PQ read one `SaturationCurve` path, and `State::path`
/// names the source. Water's exact superancillary is the answer: QT at 400 K has the curve's p there, bit for bit,
/// and PQ at that pressure the curve's root at it. A stale curve (Nitrogen edited as in
/// `stale_curve_is_a_guess_polished_by_vle`) only seeds the pure VLE: QT and PQ give `vle::at_t` and `vle::at_p`
/// from its seeds, bit for bit. A pseudo-pure fluid (R410A) answers from its ancillary definition (M6.9).
#[cfg(feature = "fluids-all")]
#[test]
fn q_pairs_report_their_saturation_source() {
    let exact = SolvePath { strategy: Strategy::Superancillary, iterations: 0 };
    let water = embedded("Water");
    let curve = record_of("Water").superancillary_curve().unwrap();
    let sat = curve.at_t(400.0).unwrap();
    let state = water.state(q_t(0.25, 400.0)).unwrap();
    assert_eq!((state.path(), state.phase(), state.quality()), (exact, Phase::TwoPhase, Some(0.25)));
    assert_eq!((state.t(), state.p()), (400.0, sat.bubble.p));
    let state = water.state(p_q(sat.bubble.p, 1.0)).unwrap();
    let root = curve.at_p(sat.bubble.p).unwrap();
    assert_eq!((state.path(), state.t(), state.p()), (exact, root.bubble.t, sat.bubble.p));
    assert!((root.bubble.t / 400.0 - 1.0).abs() < 1e-15, "{root:?}");

    let mut record = record_of("Nitrogen");
    record.eos.power.first_mut().unwrap().n *= 1.0 + 1e-6;
    let stale = record.superancillary_curve().unwrap();
    let curve_of = record.clone();
    let pure = record.builder().unwrap().lazy_saturation(move || Ok(curve_of.superancillary_curve())).build();
    let (t, guess) = (100.0, stale.at_t(100.0).unwrap());
    let vle = crate::vle::at_t(pure.eos(), t, (guess.bubble.rho, guess.dew.rho)).unwrap();
    let seed = stale.at_p(vle.dew.p).unwrap();
    let vle_p = crate::vle::at_p(pure.eos(), vle.dew.p, (seed.bubble.t, seed.bubble.rho, seed.dew.rho)).unwrap();
    let fluid = Fluid::new(Arc::new(pure));
    let polished = SolvePath { strategy: Strategy::Vle, iterations: 0 };
    let state = fluid.state(q_t(0.5, t)).unwrap();
    assert_eq!((state.path(), state.p().to_bits()), (polished, vle.dew.p.to_bits()));
    let state = fluid.state(p_q(vle.dew.p, 0.5)).unwrap();
    assert_eq!((state.path(), state.t().to_bits()), (polished, vle_p.bubble.t.to_bits()));
    assert!((vle_p.bubble.t / t - 1.0).abs() < 1e-12, "{vle_p:?}");

    let blend = embedded("R410A");
    let path = |input| blend.state(input).map(|s| s.path().strategy);
    assert_eq!((path(q_t(0.0, 280.0)), path(p_q(1e6, 0.5))), (Ok(Strategy::Ancillary), Ok(Strategy::Ancillary)));
}

/// A curve over [200 K, 380 K] with p = 10·T, ρ′ = 4000 and ρ″ = 50 mol/m³ at every T, of the given accuracy; its
/// PQ at 2500 Pa has two roots, 260 K and 280 K.
#[derive(Debug)]
struct Line(SatAccuracy);

impl SaturationCurve for Line {
    fn accuracy(&self) -> SatAccuracy {
        self.0
    }
    fn t_range(&self) -> (f64, f64) {
        (200.0, 380.0)
    }
    fn at_t(&self, t: f64) -> Result<SatPair, Error> {
        let side = |rho| SatSide { t, p: 10.0 * t, rho };
        Ok(SatPair { bubble: side(4_000.0), dew: side(50.0) })
    }
    fn at_p(&self, p: f64) -> Result<SatPair, Error> {
        if p == 2_500.0 {
            return Err(Error::Ambiguous { roots: crate::error::Roots::new(Quantity::TEMPERATURE, &[260.0, 280.0]) });
        }
        self.at_t(p / 10.0)
    }
}

/// D6 for the Q pairs (PLAN.md M6.8): a saturation state outside the model's limits (Tmin 250 K and pmax 3500 Pa
/// here) is refused by default and flagged under `Extrapolate`; inside them it is a plain two-phase state at the
/// given quality. Where p has two saturation temperatures, PQ takes the one `RootPolicy::Nearest` names and is
/// otherwise `Ambiguous`. A fluid without a curve has no Q pair, and a `Definition` curve (a pseudo-pure fluid's,
/// M6.9) no QT between Q = 0 and 1.
#[test]
fn q_pairs_check_the_domain_and_need_a_curve() {
    let fluid = |curve: Option<Line>| {
        let mut record = FluidRecord::synthetic("X").unwrap();
        record.limits = Limits::new(250.0, 420.0, 3_500.0).unwrap();
        let builder = record.builder().unwrap();
        Fluid::new(Arc::new(
            match curve {
                Some(curve) => builder.saturation(curve),
                None => builder,
            }
            .build(),
        ))
    };
    let line = fluid(Some(Line(SatAccuracy::Exact)));
    let state = line.state(q_t(0.5, 300.0)).unwrap();
    assert_eq!((state.p(), state.quality(), state.is_extrapolated()), (3_000.0, Some(0.5), false));
    assert_eq!(state.rho(Basis::Molar), 1.0 / (0.5 / 4_000.0 + 0.5 / 50.0));
    assert_eq!(line.state(p_q(3_000.0, 0.5)).map(|s| s.t()), Ok(300.0));
    let cold = DomainError::BelowMinTemperature { t: 220.0, t_min: 250.0 };
    let high = DomainError::AboveMaxPressure { p: 3_600.0, p_max: 3_500.0 };
    let outside =
        [(q_t(0.5, 220.0), cold), (p_q(2_200.0, 0.5), cold), (q_t(0.5, 360.0), high), (p_q(3_600.0, 0.5), high)];
    let extrapolate = FlashOptions::new().with_domain(DomainPolicy::Extrapolate);
    for (input, refused) in outside {
        assert_eq!(line.state(input), Err(refused.into()));
        assert!(line.flash(input, &extrapolate).unwrap().is_extrapolated());
    }
    let two = Error::Ambiguous { roots: crate::error::Roots::new(Quantity::TEMPERATURE, &[260.0, 280.0]) };
    assert_ne!(two, Error::Ambiguous { roots: crate::error::Roots::new(Quantity::TEMPERATURE, &[260.0]) });
    assert_eq!(line.state(p_q(2_500.0, 0.5)), Err(two.clone()));
    assert_eq!(line.flash(p_q(2_500.0, 0.5), &FlashOptions::new().with_roots(RootPolicy::Stable)), Err(two));
    let nearest = |t| line.flash(p_q(2_500.0, 0.5), &FlashOptions::new().with_roots(RootPolicy::Nearest(t)));
    assert_eq!(nearest(265.0).map(|s| (s.t(), s.p())), Ok((260.0, 2_500.0)));
    assert_eq!(nearest(275.0).map(|s| (s.t(), s.p())), Ok((280.0, 2_500.0)));
    let none = fluid(None);
    assert_eq!(none.state(q_t(0.5, 300.0)), Err(Error::Unsupported { pair: Pair::QT }));
    assert_eq!(none.state(p_q(3_000.0, 0.5)), Err(Error::Unsupported { pair: Pair::PQ }));
    let defined = fluid(Some(Line(SatAccuracy::Definition)));
    assert_eq!(defined.state(q_t(0.5, 300.0)), Err(Error::InvalidInput { quantity: Quantity::QUALITY, value: 0.5 }));
}

/// Issue #142 (user decision PF1): strictly inside the dome each phase carries an order-1 bundle, whose A00, A10 and
/// A01 are the order-2 bundle's bit for bit and whose second-order entries are zero; at Q = 0 and 1 it is the order-2
/// bundle. Water's and R134a's saturated liquid and vapour at 0.7 T_c; QT there is the lever rule over its Q = 0 and
/// Q = 1 states, and refuses c_p.
#[cfg(feature = "fluids-all")]
#[test]
fn inside_the_dome_the_phases_carry_order_one() {
    for name in ["Water", "R134a"] {
        let (record, fluid) = (record_of(name), embedded(name));
        let pure = record.clone().compile().unwrap();
        let t = 0.7 * record.critical.unwrap().t;
        let sat = record.superancillary_curve().unwrap().at_t(t).unwrap();
        for rho in [sat.bubble.rho, sat.dew.rho] {
            let full = total(&pure, t, rho).unwrap();
            let first = phase_bundle(&pure, t, rho, Quality::new(0.3).unwrap()).unwrap();
            assert_eq!(first, Bundle { a20: 0.0, a11: 0.0, a02: 0.0, ..full }, "{name} at {rho} mol/m³");
            for q in [0.0, 1.0] {
                assert_eq!(phase_bundle(&pure, t, rho, Quality::new(q).unwrap()).unwrap(), full, "{name} at Q = {q}");
            }
        }
        let [liquid, mid, vapour] = [0.0, 0.3, 1.0].map(|q| fluid.state(q_t(q, t)).unwrap());
        let m = Basis::Molar;
        for (x, lever) in
            [(mid.h(m), 0.7 * liquid.h(m) + 0.3 * vapour.h(m)), (mid.s(m), 0.7 * liquid.s(m) + 0.3 * vapour.s(m))]
        {
            assert!((x - lever).abs() <= 1e-12 * lever.abs().max(1.0), "{name}: {x} against {lever}");
        }
        assert!(matches!(mid.cp(m), Err(Error::Undefined { .. })), "{name}: {:?}", mid.cp(m));
    }
}
