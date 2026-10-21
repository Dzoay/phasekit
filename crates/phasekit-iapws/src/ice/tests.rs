use phasekit_core::{Basis, DataSet, DomainError, Error, Input, Pair, Phase, Prop, Registry};

use super::*;

/// Core's embedded IAPWS-95 water under `data`, and ice on it.
fn ice_on(data: DataSet) -> Fluid {
    let water = Registry::from_embedded(data).unwrap().get("Water").unwrap().clone();
    ice(&water, data).unwrap()
}

fn pt(p: f64, t: f64) -> Input {
    Input::new(Pair::PT, p, t, Basis::Mass).unwrap()
}

/// R10-06(2009) §5: from 0 K, exclusive, to 273.16 K and to 210 MPa, both ends inside; a step beyond is refused.
#[test]
fn ice_takes_its_range_and_refuses_beyond() {
    let ice = ice_on(DataSet::Corrected);
    for (p, t) in [(1e5, gibbs::T_T), (P_MAX, 200.0), (1e5, 1e-3)] {
        assert_eq!(ice.state(pt(p, t)).map(|s| s.phase()), Ok(Phase::Solid), "({p}, {t})");
    }
    let hot = DomainError::AboveMaxTemperature { t: 273.17, t_max: gibbs::T_T };
    assert_eq!(ice.state(pt(1e5, 273.17)).unwrap_err(), hot.into());
    let over = DomainError::AboveMaxPressure { p: 210.1e6, p_max: P_MAX };
    assert_eq!(ice.state(pt(210.1e6, 200.0)).unwrap_err(), over.into());
}

/// The two data sets are two models (their g₀₀ differs, DIV-0066), each on water's molar mass and gas constant;
/// PT, QT and PQ are declared, DT not; the limits state 273.16 K and 210 MPa.
#[test]
fn ice_is_a_model_of_its_own_per_data_set() {
    let (corrected, parity) = (ice_on(DataSet::Corrected), ice_on(DataSet::Parity));
    assert_ne!(corrected.model().info().key(), parity.model().info().key());
    let water = Registry::from_embedded(DataSet::Corrected).unwrap().get("Water").unwrap().clone();
    assert_eq!(corrected.model().info().molar_mass(), water.model().info().molar_mass());
    let state = corrected.state(pt(1e5, 250.0)).unwrap();
    assert_eq!(state.gas_constant(), water.model().helmholtz().unwrap().gas_constant());
    assert_eq!(corrected.model().capabilities(), Capabilities::none().with(Pair::PT).with(Pair::QT).with(Pair::PQ));
    let limits = corrected.model().limits().unwrap();
    assert_eq!((limits.t_max(), limits.p_max(), limits.t_min() > 0.0), (gibbs::T_T, P_MAX, true));
    let dt = Input::new(Pair::DT, 900.0, 250.0, Basis::Mass).unwrap();
    assert_eq!(corrected.state(dt).unwrap_err(), Error::Unsupported { pair: Pair::DT });
}

/// A water without a Helmholtz model has no gas constant to share: refused.
#[test]
fn ice_needs_a_helmholtz_water() {
    let toy = ice_on(DataSet::Corrected);
    assert_eq!(
        ice(&toy, DataSet::Corrected).unwrap_err(),
        Error::NoModel { prop: Prop::Constant(Constant::GasConstant) }
    );
}

/// The complex argument in (−π, π] on every quadrant and both axes, the origin's 0 and the negative real axis's +π
/// (Eq. (1)'s logarithms take only arguments above the real axis, so the ice states reach two quadrants alone).
#[test]
fn the_argument_covers_every_quadrant() {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI};
    let r3 = phasekit_core::math::sqrt(3.0);
    let cases = [
        ((r3, 1.0), FRAC_PI_6),
        ((-r3, 1.0), 5.0 * FRAC_PI_6),
        ((-r3, -1.0), -5.0 * FRAC_PI_6),
        ((r3, -1.0), -FRAC_PI_6),
        ((-1.0, 0.0), PI),
        ((0.0, 1.0), FRAC_PI_2),
        ((0.0, -1.0), -FRAC_PI_2),
        ((0.0, 0.0), 0.0),
    ];
    for ((re, im), want) in cases {
        let got = super::gibbs::arg(re, im);
        assert!((got - want).abs() <= 4.0 * f64::EPSILON, "arg({re}, {im}) = {got}, want {want}");
    }
}

/// Eq. (6) and its inverse over 50 K to the triple point: each T comes back from its pressure to 4 ulp, both ends
/// included, 611.657 Pa at the triple point; a pressure beyond either end is refused.
#[test]
fn the_sublimation_equation_inverts_over_its_range() {
    for k in 0..=8 {
        let t = sublimation::T_MIN + f64::from(k) * (gibbs::T_T - sublimation::T_MIN) / 8.0;
        let back = sublimation::temperature(sublimation::pressure(t).unwrap()).unwrap();
        assert!((back / t - 1.0).abs() <= 4.0 * f64::EPSILON, "{t} K comes back as {back} K");
    }
    let (p_min, p_t) = (sublimation::pressure(sublimation::T_MIN).unwrap(), 611.657);
    assert_eq!(sublimation::pressure(gibbs::T_T), Ok(p_t));
    let low = DomainError::BelowMinPressure { p: p_min / 2.0, p_min };
    let high = DomainError::AboveMaxPressure { p: 700.0, p_max: p_t };
    assert_eq!(sublimation::temperature(p_min / 2.0).unwrap_err(), low.into());
    assert_eq!(sublimation::temperature(700.0).unwrap_err(), high.into());
}
